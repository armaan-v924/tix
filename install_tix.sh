#!/usr/bin/env bash
set -euo pipefail

# Simple installer for tix. Fetches the latest GitHub release and installs the binary.

OWNER="${TIX_UPDATE_OWNER:-armaan-v924}"
REPO="${TIX_UPDATE_REPO:-tix}"
INSTALL_DIR="${TIX_INSTALL_DIR:-/usr/local/bin}"

uname_s="$(uname -s)"
uname_m="$(uname -m)"

case "${uname_s}" in
  Linux) platform="linux"; ext="tar.gz"; exe="tix" ;;
  Darwin) platform="macos"; ext="tar.gz"; exe="tix" ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT) platform="windows"; ext="zip"; exe="tix.exe" ;;
  *) echo "Unsupported OS: ${uname_s}" >&2; exit 1 ;;
esac

case "${uname_m}" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "Unsupported architecture: ${uname_m}" >&2; exit 1 ;;
esac

if [[ "${platform}" == "macos" && "${arch}" != "aarch64" ]]; then
  echo "Only macOS arm64 is supported by this installer." >&2
  exit 1
fi
if [[ "${platform}" == "windows" && "${arch}" != "x86_64" ]]; then
  echo "Only Windows x86_64 is supported by this installer." >&2
  exit 1
fi

api="https://api.github.com/repos/${OWNER}/${REPO}/releases/latest"
version="${TIX_VERSION:-$(curl -fsSL -H "Accept: application/vnd.github+json" -H "User-Agent: tix-installer" "${api}" | sed -n 's/.*"tag_name":[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' | head -n 1)}"

if [[ -z "${version}" ]]; then
  echo "Could not determine latest version from GitHub API." >&2
  exit 1
fi

asset="tix-v${version}-${platform}-${arch}.${ext}"
url="https://github.com/${OWNER}/${REPO}/releases/download/v${version}/${asset}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT

echo "Downloading ${asset}..."
curl -fL "${url}" -o "${tmpdir}/${asset}"

echo "Extracting..."
case "${ext}" in
  tar.gz) tar -xzf "${tmpdir}/${asset}" -C "${tmpdir}" ;;
  zip) unzip -q "${tmpdir}/${asset}" -d "${tmpdir}" ;;
  *) echo "Unknown archive type: ${ext}" >&2; exit 1 ;;
esac

bin_path="$(find "${tmpdir}" -type f -name "${exe}" -print -quit)"
if [[ -z "${bin_path}" ]]; then
  echo "Failed to find extracted binary '${exe}'." >&2
  exit 1
fi

mkdir -p "${INSTALL_DIR}"
cp "${bin_path}" "${INSTALL_DIR}/${exe}"
chmod +x "${INSTALL_DIR}/${exe}"

echo "tix ${version} installed to ${INSTALL_DIR}/${exe}"
echo "Make sure ${INSTALL_DIR} is on your PATH."
