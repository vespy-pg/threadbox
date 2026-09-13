#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "${repo_root}/package.json" | head -n 1)"
artifact="${repo_root}/release/linux/appimage/Threadbox_${version}_amd64.AppImage"

if [[ ! -f "${artifact}" ]]; then
  echo "Threadbox AppImage was not found at ${artifact}. Run npm run release:linux first." >&2
  exit 1
fi

data_home="${XDG_DATA_HOME:-${HOME}/.local/share}"
bin_dir="${THREADBOX_USER_BIN_DIR:-${HOME}/.local/bin}"
install_dir="${data_home}/threadbox"
applications_dir="${data_home}/applications"
icons_dir="${data_home}/icons/hicolor/128x128/apps"
installed_app="${install_dir}/Threadbox.AppImage"
launcher="${bin_dir}/threadbox"
desktop_file="${applications_dir}/Threadbox.desktop"

install -d "${install_dir}" "${bin_dir}" "${applications_dir}" "${icons_dir}"
install -m 0755 "${artifact}" "${installed_app}"
ln -sfn "${installed_app}" "${launcher}"
install -m 0644 "${repo_root}/src-tauri/icons/128x128.png" "${icons_dir}/threadbox.png"

printf '%s\n' \
  '[Desktop Entry]' \
  'Categories=Office;' \
  'Comment=A local-first work assistant for organizations and projects' \
  "Exec=${launcher}" \
  'StartupWMClass=threadbox' \
  'Icon=threadbox' \
  'Name=Threadbox' \
  'Terminal=false' \
  'Type=Application' \
  > "${desktop_file}"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "${applications_dir}" >/dev/null 2>&1 || true
fi

echo "Threadbox ${version} installed for the current user at ${installed_app}."
echo "Close any running Threadbox window and launch it again from the application menu."
