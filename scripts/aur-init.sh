#!/usr/bin/env bash
# One-shot bootstrap for the four AUR repos backing this project.
#
# For each package under packaging/, this clones the (possibly empty) AUR
# repo into a scratch directory, copies the PKGBUILD, regenerates .SRCINFO,
# commits, and pushes. Re-runs are idempotent — if nothing changed, the
# commit step is skipped.
#
# Requirements:
#   - SSH key registered on your AUR account (see ~/.ssh/aur_ci).
#   - makepkg (pacman package: pacman) available on PATH.
#
# Usage:
#   scripts/aur-init.sh                    # all four packages
#   scripts/aur-init.sh waft-git           # one package
#   AUR_SSH_KEY=~/.ssh/aur_ci scripts/aur-init.sh

set -euo pipefail

readonly REPO_ROOT="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
readonly PKG_DIR="$REPO_ROOT/packaging"
readonly WORK_DIR="${AUR_WORK_DIR:-/tmp/waft-aur}"
readonly AUR_HOST="aur@aur.archlinux.org"

ALL_PKGS=(waft-git waft-overview-git waft-settings-git waft-toasts-git)
PKGS=("${@:-${ALL_PKGS[@]}}")

if [[ -n "${AUR_SSH_KEY:-}" ]]; then
  export GIT_SSH_COMMAND="ssh -i $AUR_SSH_KEY -o IdentitiesOnly=yes"
fi

mkdir -p "$WORK_DIR"

for pkg in "${PKGS[@]}"; do
  src="$PKG_DIR/$pkg/PKGBUILD"
  if [[ ! -f "$src" ]]; then
    echo "✗ $pkg: $src not found, skipping" >&2
    continue
  fi

  echo "==> $pkg"
  dest="$WORK_DIR/$pkg"

  if [[ ! -d "$dest/.git" ]]; then
    git clone "ssh://$AUR_HOST/$pkg.git" "$dest"
  fi

  cp "$src" "$dest/PKGBUILD"

  (
    cd "$dest"
    makepkg --printsrcinfo > .SRCINFO

    git add PKGBUILD .SRCINFO
    if git diff --cached --quiet; then
      echo "    no changes"
    else
      msg="Initial import"
      if git rev-parse HEAD >/dev/null 2>&1; then
        msg="Sync PKGBUILD from monorepo"
      fi
      git commit -m "$msg"
      git push -u origin HEAD:master
    fi
  )
done

echo
echo "Done. Verify packages at:"
for pkg in "${PKGS[@]}"; do
  echo "  https://aur.archlinux.org/packages/$pkg"
done
