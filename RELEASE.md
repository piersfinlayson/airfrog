# Releasing New Version of Airfrog

## Update Version Number

To update the version:

- Add the new version to [CHANGELOG.md](CHANGELOG.md), and note key changes.
- Update the version in [airfrog-util/Cargo.toml](/airfrog-util/Cargo.toml).
- Update the version in [airfrog-bin/Cargo.toml](/airfrog-bin/Cargo.toml).
- Update the version in [airfrog-core/Cargo.toml](/airfrog-core/Cargo.toml).
- Update the version in [airfrog-swd/Cargo.toml](/airfrog-swd/Cargo.toml).
- Update the version in [airfrog/Cargo.toml](/airfrog/Cargo.toml).

## Release Process

### Preparation

Ensure all changes are committed, including the [version number updates](#update-version-number).

```bash
git pull
git push
```

Locally run the following tests:

```bash
ci/check.sh
```

### airfrog-util, airfrog-bin and airfrog-core

Publish the new version of `airfrog-util`, `airfrog-bin` and `airfrog-core` to crates.io:

```bash
cargo publish --dry-run -p airfrog-util
cargo publish --dry-run -p airfrog-bin
cargo publish --dry-run -p airfrog-core
cargo publish -p airfrog-util
cargo publish -p airfrog-bin
cargo publish -p airfrog-core
```

### airfrog-swd

Change the paths used in [`airfrog-swd`](airfrog-swd/Cargo.toml) to point to the new version of `airfrog-core` and `airfrog-bin`.

Check in changes.

Publish the new version of `airfrog-swd` to crates.io:

```bash
cargo publish --dry-run -p airfrog-swd
cargo publish -p airfrog-swd
```

### airfrog

Change the paths used in [`airfrog`](airfrog/Cargo.toml) to point to the new versions of `airfrog-util`, `airfrog-bin`, `airfrog-core` and `airfrog-util`.

Check in changes.

### Post Release

Tag the version in git:

```bash
git tag -s -a v<x.y.z> -m "Release v<x.y.z>"
git push origin v<x.y.z>
```
