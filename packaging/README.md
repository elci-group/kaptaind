# Packaging scaffolds

These are **release-time templates**, not self-publishing configuration. Nothing
in this directory is consumed by CI. The `.github/workflows/release.yml`
workflow builds the cross-platform binaries, packages them as
`kaptaind-<version>-<target>.tar.gz` (Linux/macOS) and
`kaptaind-<version>-<target>.zip` (Windows), and publishes them — together with
`SHA256SUMS.txt`, an SBOM, and cosign keyless signatures — on the GitHub
Release for tag `v<version>`. The scaffolds below consume those published
artifacts; each one is finished and published by hand at release time.

- **Homebrew** (`homebrew/kaptaind.rb`): copy the formula into a tap repository
  such as `github.com/elci-group/homebrew-tap` as `Formula/kaptaind.rb`, filling
  in `<VERSION>` and each per-target `<SHA256_*>` from `SHA256SUMS.txt`. Users
  install with `brew tap elci-group/tap && brew install kaptaind`. Manual step:
  create/maintain the tap repo and commit the filled-in formula each release.

- **Debian / APT** (`deb/build-deb.sh`): run the script against the unpacked
  Linux binaries to produce `kaptaind_<version>_<arch>.deb` (uses `dpkg-deb`,
  control depends on `git, libssl3`; set `ARCH=amd64` or `arm64`). Manual step:
  publish the `.deb` to an APT repository or Ubuntu PPA (e.g. `reprepro` into a
  hosted repo, or `dput ppa:<owner>/<ppa>`), which needs the repo/PPA and its
  signing credentials maintained outside this repo.

- **winget** (`winget/elci-group.kaptaind.yaml`): a Windows Package Manager
  manifest (schema 1.6) referencing the Windows `.zip` and its SHA256. Manual
  step: open a pull request against `microsoft/winget-pkgs` placing the filled-in
  manifest(s) under `manifests/e/elci-group/kaptaind/<version>/` (new submissions
  prefer the multi-file version/installer/locale layout; this singleton is the
  authoring starting point).
