use super::*;

pub(super) async fn run_loop(
    store: &AgentTurnStore,
    client: &LabbyAgentClient,
    turn_id: &str,
    lease_version: u64,
    approvals: &HashMap<String, String>,
    completion: CompletionFn,
) -> anyhow::Result<AgentTurnResult> {
    loop {
        let id = turn_id.to_string();
        let mut turn = persist(store, move |store| {
            store.assert_lease(&id, lease_version, now_ms())?;
            store
                .load(&id)?
                .ok_or_else(|| anyhow::anyhow!("agent_turn_not_found"))
        })
        .await?;
        if turn.cancel_requested || turn.status == AgentTurnStatus::Cancelled {
            let id = turn_id.to_string();
            return persist(store, move |store| {
                store.release_lease(&id, lease_version)?;
                store.result(&id)
            })
            .await;
        }
        if now_ms() >= turn.deadline_at_ms {
            let id = turn_id.to_string();
            return persist(store, move |store| {
                store.transition_fenced(&id, lease_version, AgentTurnStatus::TimedOut, None)?;
                store.release_lease(&id, lease_version)?;
                store.result(&id)
            })
            .await;
        }
        if turn.tool_call_count >= turn.max_tool_calls {
            let id = turn_id.to_string();
            return persist(store, move |store| {
                store.transition_fenced(
                    &id,
                    lease_version,
                    AgentTurnStatus::Failed,
                    Some("tool_budget_exceeded"),
                )?;
                store.release_lease(&id, lease_version)?;
                store.result(&id)
            })
            .await;
        }
        if let Some(pending) = turn.pending_proposal.clone() {
            let approval = approvals.get(&pending.tool_call_id).map(String::as_str);
            if pending.destructive && approval.is_none() {
                let id = turn_id.to_string();
                return persist(store, move |store| {
                    store.release_lease(&id, lease_version)?;
                    store.result(&id)
                })
                .await;
            }
            execute_proposal(&store, &client, &mut turn, pending, approval).await?;
            continue;
        }
        let id = turn_id.to_string();
        let next_status = if turn.tool_call_count == 0 {
            AgentTurnStatus::Proposing
        } else {
            AgentTurnStatus::Continuing
        };
        persist(store, move |store| {
            store.transition_fenced(&id, lease_version, next_status, None)
        })
        .await?;
        let model_prompt = build_model_prompt(&turn)?;
        let output = await_with_renewal(
            store,
            turn_id,
            lease_version,
            tokio::time::timeout(
                Duration::from_millis((turn.deadline_at_ms - now_ms()).max(1) as u64),
                completion(model_prompt),
            ),
        )
        .await
        .map_err(|_| anyhow::anyhow!("agent_deadline_exceeded"))??;
        let id = turn_id.to_string();
        let cancelled = persist(store, move |store| {
            store.assert_lease(&id, lease_version, now_ms())?;
            Ok(store.load(&id)?.is_some_and(|value| value.cancel_requested))
        })
        .await?;
        if cancelled {
            let id = turn_id.to_string();
            return persist(store, move |store| {
                store.release_lease(&id, lease_version)?;
                store.result(&id)
            })
            .await;
        }
        if output.len() > MAX_MODEL_OUTPUT_BYTES {
            anyhow::bail!("agent_model_output_too_large");
        }
        if let Some(result) = handle_model_action(
            store,
            turn_id,
            lease_version,
            approvals,
            &turn,
            parse_action(&output)?,
        )
        .await?
        {
            return Ok(result);
        }
    }
}

async fn handle_model_action(
    store: &AgentTurnStore,
    turn_id: &str,
    lease_version: u64,
    approvals: &HashMap<String, String>,
    turn: &store::StoredTurn,
    action: ModelAction,
) -> anyhow::Result<Option<AgentTurnResult>> {
    match action {
        ModelAction::Final { answer } => {
            let id = turn_id.to_string();
            let result = persist(store, move |store| {
                store.append_event_fenced(
                    &id,
                    lease_version,
                    AgentEvent::Final {
                        sequence: 0,
                        answer: answer.clone(),
                    },
                )?;
                store.transition_fenced(
                    &id,
                    lease_version,
                    AgentTurnStatus::Succeeded,
                    Some(&answer),
                )?;
                store.release_lease(&id, lease_version)?;
                store.result(&id)
            })
            .await?;
            Ok(Some(result))
        }
        ModelAction::Tool {
            tool_id,
            contract_hash,
            arguments,
            destructive,
        } => {
            let proposal = AgentToolProposal {
                tool_call_id: format!("{}:{}", turn_id, turn.tool_call_count + 1),
                tool_id,
                contract_hash,
                arguments,
                destructive,
            };
            let proposal_store = store.clone();
            let proposal_id = turn_id.to_string();
            let stored_proposal = proposal.clone();
            persist(&proposal_store, move |store| {
                store.set_proposal_fenced(&proposal_id, lease_version, &stored_proposal)
            })
            .await?;
            if destructive && !approvals.contains_key(&proposal.tool_call_id) {
                let id = turn_id.to_string();
                let result = persist(store, move |store| {
                    store.transition_fenced(
                        &id,
                        lease_version,
                        AgentTurnStatus::AwaitingApproval,
                        None,
                    )?;
                    store.release_lease(&id, lease_version)?;
                    store.result(&id)
                })
                .await?;
                return Ok(Some(result));
            }
            Ok(None)
        }
    }
}

pub(super) async fn ensure_turn(
    store: &AgentTurnStore,
    client: &LabbyAgentClient,
    turn_id: &str,
    loadout_id: &str,
    loadout_revision: u64,
    prompt: &str,
    deadline: i64,
    delegation_token: &str,
    owner: &AgentTurnOwner,
    max_tool_calls: u32,
    model: &str,
) -> anyhow::Result<()> {
    let lookup_id = turn_id.to_string();
    if let Some(existing) = persist(store, move |store| store.load(&lookup_id)).await? {
        return existing.verify_create_replay(
            &owner.principal,
            &owner.profile_id,
            loadout_id,
            loadout_revision,
            prompt,
        );
    }
    let context = client
        .create_context(delegation_token, loadout_id, loadout_revision, deadline)
        .await?;
    let turn_id = turn_id.to_string();
    let loadout_id = loadout_id.to_string();
    let prompt = prompt.to_string();
    let principal = owner.principal.clone();
    let profile_id = owner.profile_id.clone();
    let model = model.to_string();
    persist(store, move |store| {
        store.create(
            &turn_id,
            &loadout_id,
            loadout_revision,
            &prompt,
            deadline,
            &principal,
            &profile_id,
            max_tool_calls,
            &model,
            &context,
        )
    })
    .await?;
    Ok(())
}

pub(super) async fn execute_proposal(
    store: &AgentTurnStore,
    client: &LabbyAgentClient,
    turn: &mut store::StoredTurn,
    proposal: AgentToolProposal,
    approval: Option<&str>,
) -> anyhow::Result<()> {
    let lease_version = turn.version;
    let transition_id = turn.id.clone();
    persist(store, move |store| {
        store.assert_lease(&transition_id, lease_version, now_ms())?;
        store.transition_fenced(
            &transition_id,
            lease_version,
            AgentTurnStatus::Executing,
            None,
        )
    })
    .await?;
    let mut receipt = begin_execution(store, client, turn, &proposal, approval).await?;
    let cancel_check_id = turn.id.clone();
    if persist(store, move |store| {
        Ok(store
            .load(&cancel_check_id)?
            .is_some_and(|value| value.cancel_requested))
    })
    .await?
    {
        let cancelled = client.cancel(&receipt.request_id).await;
        return match cancelled {
            Ok(value) if value.status == "cancelled" => {
                let id = turn.id.clone();
                let owner = turn.owner.clone();
                persist(store, move |store| store.confirm_cancel(&id, &owner)).await?;
                Ok(())
            }
            _ => Ok(()),
        };
    }
    while receipt.status == "running" && now_ms() < turn.deadline_at_ms {
        await_with_renewal(
            store,
            &turn.id,
            lease_version,
            tokio::time::sleep(Duration::from_millis(100)),
        )
        .await;
        let poll_id = turn.id.clone();
        if persist(store, move |store| {
            Ok(store
                .load(&poll_id)?
                .is_some_and(|value| value.cancel_requested))
        })
        .await?
        {
            receipt = client.cancel(&receipt.request_id).await?;
            break;
        }
        receipt = await_with_renewal(
            store,
            &turn.id,
            lease_version,
            client.status(&receipt.request_id),
        )
        .await?;
    }
    let final_turn_id = turn.id.clone();
    persist(store, move |store| {
        store.record_receipt_fenced(&final_turn_id, lease_version, &proposal, &receipt)?;
        match receipt.status.as_str() {
            "succeeded" => store.complete_tool_fenced(
                &final_turn_id,
                lease_version,
                &proposal.tool_call_id,
                receipt.result.unwrap_or(Value::Null),
            ),
            "running" => store.transition_fenced(
                &final_turn_id,
                lease_version,
                AgentTurnStatus::TimedOut,
                Some("labby_status_deadline"),
            ),
            "cancelled" => store.transition_fenced(
                &final_turn_id,
                lease_version,
                AgentTurnStatus::Cancelled,
                Some("labby_cancelled"),
            ),
            "timed_out" => store.transition_fenced(
                &final_turn_id,
                lease_version,
                AgentTurnStatus::TimedOut,
                Some("labby_timed_out"),
            ),
            _ => store.transition_fenced(
                &final_turn_id,
                lease_version,
                AgentTurnStatus::Failed,
                receipt.error_kind.as_deref(),
            ),
        }
    })
    .await?;
    Ok(())
}

async fn begin_execution(
    store: &AgentTurnStore,
    client: &LabbyAgentClient,
    turn: &store::StoredTurn,
    proposal: &AgentToolProposal,
    approval: Option<&str>,
) -> anyhow::Result<LabbyExecutionReceipt> {
    let lease_version = turn.version;
    let key = format!("axon-agent:{}:{}", turn.id, proposal.tool_call_id);
    let request_turn_id = turn.id.clone();
    let request_tool_call_id = proposal.tool_call_id.clone();
    let request_id = persist(store, move |store| {
        store.execution_request_id(&request_turn_id, &request_tool_call_id)
    })
    .await?;
    let receipt = match request_id {
        Some(request_id) => {
            await_with_renewal(store, &turn.id, lease_version, client.status(&request_id)).await?
        }
        None => {
            let reserve_turn_id = turn.id.clone();
            let reserve_tool_call_id = proposal.tool_call_id.clone();
            let reserve_key = key.clone();
            persist(store, move |store| {
                store.reserve_execution_fenced(
                    &reserve_turn_id,
                    lease_version,
                    &reserve_tool_call_id,
                    &reserve_key,
                )
            })
            .await?;
            await_with_renewal(
                store,
                &turn.id,
                lease_version,
                client.execute(
                    &turn.execution_context_id,
                    &key,
                    proposal,
                    approval,
                    turn.deadline_at_ms,
                ),
            )
            .await?
        }
    };
    let turn_id = turn.id.clone();
    let tool_call_id = proposal.tool_call_id.clone();
    let request_id = receipt.request_id.clone();
    let reconciled = persist(store, move |store| {
        store.reconcile_dispatched_request(&turn_id, &tool_call_id, &key, &request_id)
    })
    .await?;
    anyhow::ensure!(reconciled, "labby_request_reconciliation_failed");
    Ok(receipt)
}
