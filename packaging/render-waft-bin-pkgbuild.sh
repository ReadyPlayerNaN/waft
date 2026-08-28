#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <version> <sha256_x86_64> <output-path>" >&2
  exit 1
fi

version=$1
sha256_x86_64=$2
output_path=$3
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
template_path="$script_dir/waft-bin/PKGBUILD.in"

mkdir -p "$(dirname -- "$output_path")"
sed \
  -e "s|@PKGVER@|$version|g" \
  -e "s|@SHA256_X86_64@|$sha256_x86_64|g" \
  "$template_path" > "$output_path"
