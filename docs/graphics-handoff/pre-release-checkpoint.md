# Pre-release review checkpoint

Implementation: `claude/branch-review-opz967`. Review base: `f925f608`.
Reference main: `dd4529e2935b07d7a5bf8deb764421e4d201fc41`; its backend/Cargo
source matches `v2.2.1`. Status: **NOT READY**. The user requested a checkpoint
push before the remaining weekly budget is consumed. No release is prepared.

## Implemented scope

- Localize recognized conversion-default model/layer names for display only;
  preserve raw saved names, IDs, custom names and repeated conversion.
- Preserve exact Composer geometry when compatibility values are unchanged,
  including an output resize. Adopt surviving schema-29/30 bindings/scenes
  when an intermediate marker is absent; do not edit installed migrations.
- Bound manual expansion and queued takes; invalidate stale work on authored
  edits/output changes, preserve waiting commands on ordinary visibility
  controls, and serialize final publication with lifecycle cancellation.
- Prepare model artwork outside frame-critical locks; cache logo movement,
  preserve animation epochs and couple filtered image targets to emitted specs.
- Isolate invalid optional permanent artwork while propagating database faults.
- Commit new/edited template content and optional assignment in one transaction.
- Guard dirty navigation and asynchronous editor completions; correct natural
  image restore, handle placement, keyboard controls, focus and on-air polling.
- Add opt-in native `qa/soak` execution, model cycling and a separate video ROI
  so moving overlays cannot hide frozen program pictures.

See [ownership and concurrency](../graphics-maintenance.md) and
[the API contract](../api.md#graphics-templates). Original attachments and
independent reports are on
[evidence/graphics-2.3](https://github.com/LuizStSantos/NoAr/tree/evidence/graphics-2.3/docs/graphics-handoff/pre-release-2026-11-09).
The implementation handoff retains 20 principal images; original prototypes,
logs, large JSON, archives and repeated captures remain on the evidence branch.

## Verification ledger

| Check | Status at checkpoint |
| --- | --- |
| Migration 00029–00034 bytes versus f925f608 | PASS: all six SHA-256 values unchanged |
| Initial Rust checks before peer-challenge fixes | PASS: fmt, no-default Clippy, 747 tests; 14 ignored; engine processing-bench tests passed separately |
| Final Rust checks after peer-challenge fixes | PASS: fmt, no-default Clippy/all-targets/deny warnings; 757 tests passed, 14 ignored |
| Vue types, changed-file ESLint/Prettier, E2E TypeScript | PASS after frontend source fixes |
| Initial browser suite | 42/45 passed; handle sizing, restore and one test selector failed; source corrections and expanded regressions added |
| Final expanded browser suite | NOT_RUN at this checkpoint; 48 tests prepared against fresh owned staging |
| Soak harness/observer unit tests | PASS: 25 tests |
| 90-minute model/graphics soak | NOT_RUN |
| Main UDP/SRT reference runs | Executed: 125 seconds each, common 2.2.1 APIs, same synthetic media and local runtime |
| Candidate UDP/SRT comparison | NOT_RUN |
| 1080p25 processing matrix | NOT_RUN; model blend instrumentation added, capacity not established |
| Backend/frontend councils | Five independent Round A and Round B reports per council; Round C ballots NOT_RUN; no valid council verdict |

The main UDP audio-only fault produced 23.8 seconds of an exact unchanged decoded
ROI; an extracted output frame is black. This establishes the static-output
symptom on 2.2.1, not equal branch recovery timing. The main SRT run did not
reproduce the reported 1-second pacing-reset warning. Its cause remains
unresolved until the candidate/common-scenario comparison is complete. Healthy
`/health` responses do not prove moving pictures or synchronization.

Initial browser failure screenshots/traces were retained locally. A discovery
command overwrote one initial JSON reporter output; no complete initial report
is asserted. A later attempt used an expired staging session and is not valid
product evidence. The final run must initialize a fresh database/session.

## Resume before release preparation

1. Finish final Rust and 48-test browser checks, inspect the localized dark
   desktop/mobile captures, and archive actual results. Fix failures first.
2. Repeat the matched UDP/SRT scenarios with the final candidate binary;
   investigate any new degradation and the unclassified SRT pacing reset.
3. Measure processing-bench at 1080p25 with 0/1/4/16 GCs, L-shape and scrolling
   text. Main has no model GC/scroll API: mark unsupported cells unavailable,
   compare common paths directly and report candidate incremental costs.
4. Run `qa/soak` for 90 real minutes with `NOAR_SOAK_GRAPHICS=1` using the final
   binary, independently observing moving program pixels. Disclose the existing
   bounded startup/restart/SRT/UDP fault grace periods; preserve logs/receipts.
5. Send final fixes and executed evidence to each council's five independent
   voters for Round C. Tally each council separately; advisory votes do not
   replace the runtime gates or authorize a release.
6. Refresh the handoff/map and commit/push only the authorized branches.
