# Specialist ↔ Orchestrator handoff rules

- `noar-ffmpeg-docs-specialist` is a **non-voting technical adviser**, not an eleventh council voter.
- It can be invoked by the main session, the Orchestrator, or the Media Engine Reviewer when FFmpeg-specific documentation is needed. Never duplicate the same research across agents.
- It reads only relevant official references and bounded source ranges. It returns citations, compatibility notes, unknowns, and suggested synthetic-media tests.
- `noar-media-engine-reviewer` remains an independent voting member of the Backend Council. Research from the adviser is evidence to be independently checked when casting a vote.
- `noar-integration-e2e-qa` performs actual staging verification. A documentation citation alone is not a passing integration test.
- `noar-documentation-guardian` receives a concise final change summary, documentation impact and official source links. It never continuously observes all agents.
- A change that risks on-air continuity cannot be approved based on vote tally alone; required runtime validation must be recorded separately.

Example Orchestrator delegation:

```text
Consult noar-ffmpeg-docs-specialist for official, version-aware references on timestamp rescaling in encoded HLS output. Inspect only backend/engine/src/output/encoded.rs and the relevant helper functions. Return a maximum of four reference-backed findings and staging tests. Do not implement or vote.
```
