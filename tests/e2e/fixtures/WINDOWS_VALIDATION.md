# Windows validation ownership

The portable isolation contract includes native Windows process creation-time,
process-group, manifest-key DACL, and lock-directory DACL paths. Unit tests use
injected Windows API and `icacls` results so those branches remain executable on
non-Windows development hosts.

Running these contracts on a real Windows runner, including subprocess signal
and path-semantics smoke coverage, belongs to platform bead
`axon_rust-nnzde.22`. Hermetic CI must fail unavailable until that runner proves
the native path; the portable core does not silently fall back to POSIX logic.
