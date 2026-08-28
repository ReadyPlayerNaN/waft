# Release process

Waft ships two AUR package tracks:

- `waft-git` — VCS package built from source by AUR users
- `waft-bin` — stable package that repackages prebuilt GitHub Release artifacts

## Why `waft-bin`

The stable AUR package downloads compiled release artifacts rather than building from a versioned source tarball inside `makepkg`.
Per normal AUR naming guidance, that makes it a `-bin` package rather than an unsuffixed `waft` package.

## Release artifact format

Each stable release publishes an immutable archive named:

- `waft-<version>-x86_64.tar.gz`
- `waft-<version>-x86_64.tar.gz.sha256`

The archive contains:

- `bin/waft`
- `bin/waft-overview`
- `bin/waft-settings`
- `bin/waft-launcher`
- all bundled `waft-*-daemon` plugin binaries
- `share/dbus-1/services/org.waft.Daemon.service`
- `lib/systemd/user/waft.service`
- `share/applications/waft-settings.desktop`
- `LICENSE`

## Automated workflow chain

### Regular development on `master`

1. `ci.yml` runs tests.
2. `packaging.yml` validates AUR recipes after successful CI.
3. `aur-publish.yml` publishes `waft-git` after successful packaging validation on `master`.

### Stable release

1. Merge the intended release contents to `master`.
2. Create and push a semver tag:
   ```bash
   git tag -a vX.Y.Z -m "waft vX.Y.Z"
   git push origin vX.Y.Z
   ```
3. `release.yml` runs on the tag:
   - validates the release build with tests
   - builds release binaries in an Arch container
   - assembles `waft-<version>-x86_64.tar.gz`
   - generates `sha256`
   - publishes the GitHub Release and assets
4. Publishing the GitHub Release triggers `aur-publish.yml` for `waft-bin`.
5. The `waft-bin` publish job:
   - fetches the checksum from the immutable GitHub Release asset
   - renders `packaging/waft-bin/PKGBUILD` from `PKGBUILD.in`
   - publishes the resulting recipe to AUR

## Manual recovery / republish

### Rebuild a GitHub Release for an existing tag

Use `release.yml` via `workflow_dispatch` and provide the existing tag, for example `v1.2.3`.

### Republish `waft-bin` to AUR for an existing release

Use `aur-publish.yml` via `workflow_dispatch` with:

- `package = waft-bin`
- `tag = vX.Y.Z`

### Republish `waft-git`

Use `aur-publish.yml` via `workflow_dispatch` with:

- `package = waft-git`

## Packaging notes

- `packaging/waft-bin/PKGBUILD.in` is the tracked template.
- `packaging/render-waft-bin-pkgbuild.sh` renders a concrete `PKGBUILD` from a version and checksum.
- `packaging.yml` renders `waft-bin` with dummy version/checksum values so the template is syntax-checked and linted in CI without requiring a live release asset.
