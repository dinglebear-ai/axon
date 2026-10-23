//! Preserve admission/order while removing repeated input scans.
use super::*;
use std::time::Instant;

fn legacy_estimate(text: &str) -> usize {
    text.chars()
        .fold(0_usize, |n, ch| {
            n + if ch.is_ascii() { 1 } else { ch.len_utf8() }
        })
        .max(1)
}

#[test]
fn byte_weight_matches_legacy_for_every_unicode_scalar_and_mixed_text() {
    assert_eq!(estimated_tokens(""), legacy_estimate(""));
    for c in (0..=0x10ffff).filter_map(char::from_u32) {
        let mut buffer = [0_u8; 4];
        let text = c.encode_utf8(&mut buffer);
        assert_eq!(estimated_tokens(text), legacy_estimate(text));
    }
    for text in ["code\r\n\0λ漢字😀", "a\u{301}é\t x", "\u{10ffff}\u{feff}"] {
        assert_eq!(estimated_tokens(text), legacy_estimate(text));
    }
}

#[test]
fn cached_keys_preserve_ties_unicode_and_all_batch_boundaries() {
    let inputs = ["", "a", "λ", "ab", "漢", "abcd", "😀", "aaaa"].map(String::from);
    let batches = pack_batches(
        &inputs,
        BatchLimits {
            max_inputs: 3,
            max_input_tokens: 8,
            max_batch_tokens: 6,
            max_batch_bytes: 64,
        },
    )
    .unwrap();
    let indices = batches
        .iter()
        .map(|(ids, _)| ids.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        indices,
        vec![vec![0, 1, 2], vec![3, 4], vec![5], vec![6], vec![7]]
    );
    for (ids, texts) in batches {
        assert!(ids.len() <= 3);
        assert!(texts.iter().map(|s| legacy_estimate(s)).sum::<usize>() <= 6);
        assert!(
            32 + texts
                .iter()
                .map(|s| serialized_input_bytes(s))
                .sum::<usize>()
                <= 64
        );
        for (id, text) in ids.into_iter().zip(texts) {
            assert_eq!(text, inputs[id]);
        }
    }
}

fn inputs() -> Vec<String> {
    (0..512)
        .map(|i| "fn read_code() { λ漢字😀 }\n".repeat(8 + (i * 37 % 250)))
        .collect()
}

#[test]
#[ignore = "manual CPU benchmark; no timing threshold in CI"]
fn measure_cached_packing_keys_against_repeated_scans() {
    let inputs = inputs();
    let mut totals = [0_u128; 2];
    for iteration in 0..20 {
        let mut ordered_outputs = Vec::new();
        for mode in [iteration % 2, (iteration + 1) % 2] {
            let mut ordered = inputs.iter().enumerate().collect::<Vec<_>>();
            let start = Instant::now();
            if mode == 0 {
                ordered.sort_by_key(|(index, text)| (legacy_estimate(text), text.len(), *index));
            } else {
                let mut keyed = ordered
                    .into_iter()
                    .map(|(index, text)| (estimated_tokens(text), text.len(), index, text))
                    .collect::<Vec<_>>();
                keyed.sort_by_key(|&(tokens, bytes, index, _)| (tokens, bytes, index));
                ordered = keyed
                    .into_iter()
                    .map(|(_, _, index, text)| (index, text))
                    .collect();
            }
            totals[mode] += start.elapsed().as_micros();
            ordered_outputs.push(ordered.iter().map(|(i, _)| *i).collect::<Vec<_>>());
            std::hint::black_box(ordered);
        }
        assert_eq!(ordered_outputs[0], ordered_outputs[1]);
    }
    eprintln!(
        "packing-key benchmark: 20 x 512 mixed-length Unicode inputs; repeated_scan_us={} cached_key_us={} ratio={:.3}",
        totals[0],
        totals[1],
        totals[0] as f64 / totals[1] as f64
    );
}
#[test]
fn packing_accounts_for_escaped_json_bytes_and_envelope() {
    let inputs = vec![String::from("\"\n\0"); 3];
    let batches = pack_batches_counted(
        &inputs,
        &[1, 1, 1],
        BatchLimits {
            max_inputs: 3,
            max_input_tokens: 8,
            max_batch_tokens: 8,
            max_batch_bytes: 64,
        },
    )
    .unwrap();
    assert_eq!(
        batches
            .iter()
            .map(|(ids, _)| ids.clone())
            .collect::<Vec<_>>(),
        vec![vec![0, 1], vec![2]]
    );
    for (_, texts) in batches {
        assert!(
            32 + texts
                .iter()
                .map(|text| serialized_input_bytes(text))
                .sum::<usize>()
                <= 64
        );
        for text in texts {
            assert_eq!(
                serialized_input_bytes(text),
                serde_json::to_vec(text).unwrap().len() + 1
            );
        }
    }
}

#[test]
fn counted_packing_uses_byte_length_then_original_index_for_ties() {
    let inputs = ["漢", "ab", "λ", "abc"].map(String::from);
    let batches = pack_batches_counted(
        &inputs,
        &[1; 4],
        BatchLimits {
            max_inputs: 4,
            max_input_tokens: 8,
            max_batch_tokens: 8,
            max_batch_bytes: 64,
        },
    )
    .unwrap();
    assert_eq!(batches[0].0, vec![1, 2, 0, 3]);
}
