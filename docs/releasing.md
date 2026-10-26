**Versions and releases**
================

NoAr's version is `MAJOR.MINOR.PATCH` ([Semantic Versioning](https://semver.org)),
set once in the workspace `Cargo.toml` and repeated in `package.json`. Every
release is a git tag `vMAJOR.MINOR.PATCH` with its notes in
[CHANGELOG.md](../CHANGELOG.md).

## What each number means

For a playout, "breaking" means anything an operator or an integration has to
act on when they update:

| Part | Goes up when | Examples |
| --- | --- | --- |
| **MAJOR** | An update needs the operator to do something, or something that worked stops working. | An API endpoint or field removed or renamed. A playlist, `.nfo` or `.logo.json` format that older files no longer satisfy. A database migration that cannot be undone by going back to the previous image. A setting removed or its meaning changed. A required change to `docker-compose.yml`. |
| **MINOR** | Something new, and everything that worked keeps working. | A new page, setting, API endpoint or optional field. A database migration that only adds tables or columns. |
| **PATCH** | Only fixes, with no new behaviour to learn. | A bug fix, a translation, a performance improvement. |

A release candidate for a larger change can be tagged `v1.2.0-rc.1`. Images
built from it are never published as `latest`.

## Making a release

Releases are prepared by a workflow, and published when a person merges its
pull request:

1. **Actions › Prepare release › Run workflow**. Leave **bump** on `auto`, or
   pick `patch`, `minor` or `major`; **pre** makes a release candidate
   (`rc.1`). It works out the next version from the **Unreleased** notes —
   `### Removed`, `### Breaking` or a `**Breaking**` entry make a MAJOR,
   `### Added` or a new migration since the last tag a MINOR, anything else a
   PATCH — then dates the notes as `## [X.Y.Z] - YYYY-MM-DD`, updates the
   links at the bottom, sets the version in `Cargo.toml`, `Cargo.lock`,
   `package.json` and `package-lock.json`, and opens the pull request
   `Release X.Y.Z` with Validate running on it.
2. Review the notes and the number in that pull request, and merge it when
   CI is green.
3. **Tag release** then tags the merge commit `vX.Y.Z` and starts the release
   and image workflows for it.

The same steps run locally with `python3 .claude/scripts/prepare_release.py`
(`--check` only prints the plan), and the `noar-release-manager` agent
(`/noar-intelligent-development release`) checks the notes against the
commits, runs the gates and prepares the pull request. A tag pushed by hand
(`git tag -a vX.Y.Z -m "NoAr X.Y.Z" && git push origin vX.Y.Z`) still
releases as before.

Prepare release needs *Allow GitHub Actions to create and approve pull
requests* checked under **Settings › Actions › General › Workflow
permissions**.

For a tag, the workflows then:

- run Validate (tests, lints, build) and stop if it fails;
- check that the tag matches the version in `Cargo.toml`, and stop if it does
  not;
- publish the image as `ghcr.io/luizstsantos/noar:X.Y.Z` and `:X.Y`;
- create the GitHub release with the tag's section of the changelog.

## Image tags

| Tag | What it is |
| --- | --- |
| `latest` | The newest stable release (never a release candidate). This is what `docker-compose.yml` uses, what the update notice compares with and what **Update now** installs. |
| `X.Y.Z` | One release, never rebuilt. |
| `X.Y` | The newest patch of a minor release. Pin this to get fixes only; the updater then stays on `X.Y`. |
| `main` | The newest build of `main`, for following development. Not for production. |
| `sha-abcdef0` | One commit, from `main` or a release, to go back to an exact build. |

`latest` moves only when a release is published. To stay on one minor
release and get only its fixes, set the image in `docker-compose.yml` to
`ghcr.io/luizstsantos/noar:1.1` (or `:1.1.0` to stay on one release).

## Where the version shows

Settings › Updates shows the running version and the commit it was built from.
`noar --version` prints it too.
