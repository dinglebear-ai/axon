# GitLab Ingest
Last Modified: 2026-05-21

GitLab ingest indexes project metadata, repository files, issues, merge requests, and wiki pages into Qdrant.

## Targets

```bash
axon https://gitlab.com/gitlab-org/gitlab-runner --wait true
axon https://gitlab.com/group/subgroup/project --wait true
axon gitlab:gitlab.example.com/group/project --wait true
```

Bare `owner/repo` remains GitHub shorthand. Use a full GitLab URL or the `gitlab:` prefix for GitLab targets, especially for self-hosted instances.

## Authentication

`GITLAB_TOKEN` is optional for public GitLab projects and required for private projects. The token is sent to the GitLab REST API with the `PRIVATE-TOKEN` header. Repository clone authentication is passed through a temporary Git HTTP header so the token is not embedded in the clone URL.

## Indexed Content

| Content | Notes |
|---------|-------|
| Project metadata | Name, description, visibility, stars, forks, default branch, feature flags |
| Files | Documentation files always; source files unless `--no-source` is set |
| Issues | Sorted by most recently updated; limited by `--max-issues` |
| Merge requests | Sorted by most recently updated; limited by `--max-prs` |
| Wiki | Uses the GitLab Project Wikis API with `with_content=1` |

## Limits

```bash
axon https://gitlab.com/group/project --max-issues 25 --max-prs 25
axon https://gitlab.com/group/project --no-source
```

The existing ingest flags are shared with GitHub: `--max-issues`, `--max-prs`, `--no-source`, and `--include-source`.
