#!/bin/sh
set -eu

EX_UNAVAILABLE=78
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)

fail_unavailable() {
    printf '%s\n' "$*" >&2
    exit "$EX_UNAVAILABLE"
}

if [ "$(uname -s)" != "Linux" ]; then
    fail_unavailable "Artix L0/L2 fact collection requires Linux"
fi

if [ ! -r /etc/os-release ]; then
    fail_unavailable "Artix L0/L2 fact collection requires readable /etc/os-release"
fi

# shellcheck disable=SC1091
. /etc/os-release
if [ "${ID:-}" != "artix" ]; then
    fail_unavailable "Artix L0/L2 fact collection refuses non-Artix host: ID=${ID:-unknown}"
fi

export LC_ALL=C

for required in pacman awk sed grep stat git uname rc-status rc-service rc-update; do
    if ! command -v "$required" >/dev/null 2>&1; then
        fail_unavailable "Artix L0/L2 fact collection requires command: $required"
    fi
done

one_line() {
    awk 'BEGIN { first = 1 } { if (!first) printf " ; "; printf "%s", $0; first = 0 } END { printf "\n" }'
}

command_version() {
    name=$1
    shift
    if command -v "$name" >/dev/null 2>&1; then
        version=$({ "$name" "$@" 2>&1 || true; } | one_line)
        printf '%s\t%s\t%s\n' "$name" "$(command -v "$name")" "$version"
    else
        printf '%s\t%s\t%s\n' "$name" "missing" "missing"
    fi
}

package_probe() {
    pkg=$1
    if info=$(LC_ALL=C pacman -Si "$pkg" 2>/dev/null); then
        repo=$(printf '%s\n' "$info" | awk -F ' *: *' '$1 == "Repository" { print $2; exit }')
        version=$(printf '%s\n' "$info" | awk -F ' *: *' '$1 == "Version" { print $2; exit }')
        arch=$(printf '%s\n' "$info" | awk -F ' *: *' '$1 == "Architecture" { print $2; exit }')
        licenses=$(printf '%s\n' "$info" | awk -F ' *: *' '$1 == "Licenses" { print $2; exit }')
        installed=absent
        if installed_line=$(LC_ALL=C pacman -Q "$pkg" 2>/dev/null); then
            installed=$installed_line
        fi
        printf '%s\tavailable\t%s\t%s\t%s\t%s\t%s\n' "$pkg" "$repo" "$version" "$arch" "$licenses" "$installed"
    else
        printf '%s\tmissing\t-\t-\t-\t-\tabsent\n' "$pkg"
    fi
}

printf '%s\n' '[artix-host]'
printf 'captured_at_utc\t%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
printf 'os_id\t%s\n' "${ID:-}"
printf 'os_name\t%s\n' "${NAME:-}"
printf 'os_pretty_name\t%s\n' "${PRETTY_NAME:-}"
printf 'os_version\t%s\n' "${VERSION:-}"
printf 'architecture\t%s\n' "$(uname -m)"
printf 'kernel\t%s\n' "$(uname -r)"
printf 'kernel_full\t%s\n' "$(uname -a)"
if [ -r /sys/class/dmi/id/sys_vendor ]; then
    printf 'dmi_vendor\t%s\n' "$(sed -n '1p' /sys/class/dmi/id/sys_vendor)"
fi
if [ -r /sys/class/dmi/id/product_name ]; then
    printf 'dmi_product\t%s\n' "$(sed -n '1p' /sys/class/dmi/id/product_name)"
fi
set -- $(stat -f -c '%S %b %a' "$repo_root")
printf 'repo_filesystem_block_size\t%s\n' "$1"
printf 'repo_filesystem_blocks_total\t%s\n' "$2"
printf 'repo_filesystem_blocks_available\t%s\n' "$3"

printf '\n%s\n' '[repository]'
printf 'repo_root\t%s\n' "$repo_root"
printf 'git_revision\t%s\n' "$(git -C "$repo_root" rev-parse HEAD)"
if [ -z "$(git -C "$repo_root" status --porcelain)" ]; then
    printf '%s\n' 'git_clean\ttrue'
else
    printf '%s\n' 'git_clean\tfalse'
fi
printf 'git_branch\t%s\n' "$(git -C "$repo_root" branch --show-current)"
printf 'git_origin\t%s\n' "$(git -C "$repo_root" remote get-url origin 2>/dev/null || printf '%s' 'missing')"

printf '\n%s\n' '[toolchain]'
printf '%s\n' 'name\tpath\tversion'
command_version pacman --version
command_version git --version
command_version rustc --version
command_version cargo --version
command_version buildiso --version
command_version calamares --version
command_version rc-status --version
command_version rc-service --version
command_version rc-update --version

printf '\n%s\n' '[pacman-repositories]'
awk '
    /^[[:space:]]*\[[^]]+\][[:space:]]*$/ {
        section=$0
        gsub(/^[[:space:]]*\[/, "", section)
        gsub(/\][[:space:]]*$/, "", section)
        if (section != "options") print section
    }
' /etc/pacman.conf

printf '\n%s\n' '[pacman-mirror-sources]'
for mirror_file in /etc/pacman.d/*mirrorlist*; do
    if [ -r "$mirror_file" ]; then
        printf 'file\t%s\n' "$mirror_file"
        awk '
            /^[[:space:]]*#/ { next }
            /^[[:space:]]*$/ { next }
            { print }
        ' "$mirror_file"
    fi
done

printf '\n%s\n' '[package-candidates]'
printf '%s\n' 'package\tavailability\trepository\tversion\tarchitecture\tlicenses\tinstalled'
for pkg in \
    base base-devel git rust curl ca-certificates \
    linux linux-lts linux-firmware intel-ucode amd-ucode \
    grub efibootmgr cryptsetup lvm2 e2fsprogs dosfstools mkinitcpio \
    dbus dbus-openrc elogind elogind-openrc \
    networkmanager networkmanager-openrc openssh openssh-openrc \
    nftables nftables-openrc chrony chrony-openrc syslog-ng syslog-ng-openrc \
    xorg-server xorg-xinit xorg-xprop i3-wm alacritty tmux \
    bubblewrap chromium xdg-utils nodejs npm calamares \
    artools-base artools-iso artools-pkg artix-live-base artix-live-openrc artix-keyring artix-mirrorlist \
    open-vm-tools open-vm-tools-openrc xf86-video-vmware mesa \
    maim scrot xdotool polkit polkit-openrc
 do
    package_probe "$pkg"
done

printf '\n%s\n' '[openrc-current-state]'
printf '%s\n' '-- rc-status -a --'
rc-status -a 2>&1 || true
printf '%s\n' '-- rc-update show --'
rc-update show 2>&1 || true

printf '\n%s\n' '[candidate-init-scripts]'
for service in \
    dbus elogind NetworkManager networkmanager sshd nftables \
    chronyd chrony syslog-ng
 do
    path="/etc/init.d/$service"
    if [ -e "$path" ]; then
        mode=$(stat -c '%a' "$path" 2>/dev/null || printf '%s' '?')
        owner=$(stat -c '%U:%G' "$path" 2>/dev/null || printf '%s' '?')
        printf '%s\tpresent\t%s\t%s\n' "$service" "$mode" "$owner"
    else
        printf '%s\tabsent\t-\t-\n' "$service"
    fi
done

printf '\n%s\n' '[notes]'
printf '%s\n' 'This collector is read-only. Package availability is evidence, not package selection.'
printf '%s\n' 'Missing candidate names must be researched; they must not be replaced with AUR packages automatically.'
printf '%s\n' 'Service identities/runlevels become resolved only after the selected packages are installed and their real OpenRC scripts are inspected/exercised.'
