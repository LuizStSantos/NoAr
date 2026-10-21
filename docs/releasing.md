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

1. On a branch, move everything under **Unreleased** in `CHANGELOG.md` into a
   new section `## [X.Y.Z] - YYYY-MM-DD`, and update the links at the bottom.
2. Set the version in `Cargo.toml` (`[workspace.package] version`), in
   `package.json`, and in the root entries of `package-lock.json`. Then run
   `cargo build` so `Cargo.lock` follows.
3. Open a pull request named `Release X.Y.Z`, and merge it when CI is green.
4. Tag the merge commit and push the tag:

   ```bash
   git checkout main && git pull
   git tag -a vX.Y.Z -m "NoAr X.Y.Z"
   git push origin vX.Y.Z
   ```

For a tag, the workflows then:

- check that the tag matches the version in `Cargo.toml`, and stop if it does
  not;
- publish the image as `ghcr.io/luizstsantos/noar:X.Y.Z` and `:X.Y`;
- create the GitHub release with the tag's section of the changelog.

## Image tags

| Tag | What it is |
| --- | --- |
| `latest` | The newest build of `main`. This is what `docker-compose.yml` uses and what the update notice compares with. |
| `X.Y.Z` | One release, never rebuilt. |
| `X.Y` | The newest patch of a minor release. Pin this to get fixes only. |
| `sha-abcdef0` | One commit of `main`. |

To stay on a release instead of following `main`, set the image in
`docker-compose.yml` to `ghcr.io/luizstsantos/noar:1.0` (or `:1.0.0`).

## Where the version shows

Settings › Updates shows the running version and the commit it was built from.
`noar --version` prints it too.
