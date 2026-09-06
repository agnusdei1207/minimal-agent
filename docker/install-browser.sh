#!/usr/bin/env bash
# This script is copied into a Linux image and must retain LF line endings.
set -euo pipefail

CHROME_FOR_TESTING_VERSION="${1:?browser version is required}"
CHROME_FOR_TESTING_SHA256="${2:?browser SHA-256 is required}"

if [[ "$(dpkg --print-architecture)" != "amd64" ]]; then
  echo "Chrome for Testing bundle supports amd64 only" >&2
  exit 1
fi

download_dir="$(mktemp -d)"
trap 'rm -rf "${download_dir}"' EXIT
archive="${download_dir}/chrome-linux64.zip"
url="https://storage.googleapis.com/chrome-for-testing-public/${CHROME_FOR_TESTING_VERSION}/linux64/chrome-linux64.zip"

curl --fail --location --retry 3 --retry-all-errors --output "${archive}" "${url}"
printf '%s  %s\n' "${CHROME_FOR_TESTING_SHA256}" "${archive}" | sha256sum --check --strict

install_root="/opt/minimal-agent-browser/${CHROME_FOR_TESTING_VERSION}"
mkdir --parents "${install_root}"
unzip -q "${archive}" -d "${install_root}"
browser="${install_root}/chrome-linux64/chrome"
sandbox="${install_root}/chrome-linux64/chrome_sandbox"
test -x "${browser}"
test -f "${sandbox}"
chown root:root "${sandbox}"
chmod 4755 "${sandbox}"

ln -s "${browser}" /usr/local/bin/minimal-agent-browser
minimal-agent-browser --version

cat << 'EOF' > /usr/local/bin/google-chrome
#!/usr/bin/env bash
echo "[ERROR] Direct invocation of raw google-chrome/chromium is disabled to prevent alert() modal lockups (ADR-0005)." >&2
echo "[ADR-0005] For web challenges, send direct HTTP payloads via curl or Python requests." >&2
echo "           For automated browser rendering, use agent-browser or minimal-agent-browser with alert neutralization." >&2
exit 1
EOF
chmod 0755 /usr/local/bin/google-chrome
ln -sf /usr/local/bin/google-chrome /usr/local/bin/chromium
