# Kiln GNU/Linux x86_64 ISO builder: Debian 13 (trixie) amd64 with everything needed to
#   1. compile Glaze (Rust) and the Kiln apps (C, Vala, C++, Rust, Python), and
#   2. run live-build to produce the hybrid BIOS+UEFI live ISO.
# Build:  docker build --platform linux/amd64 -t kiln-builder build/x86
# Use via build/x86/build-iso.sh (see build/README.md). Needs `docker run --privileged` (debootstrap/live-build mount filesystems).
FROM --platform=linux/amd64 debian:13

ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8 \
    RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH

# deb-src is needed for `apt-get build-dep` of the apps Kiln forks
RUN sed -i 's/^Types: deb$/Types: deb deb-src/' /etc/apt/sources.list.d/debian.sources \
 && apt-get update

# --- toolchain, generic build tools, live-build and its host tools ---
RUN apt-get install -y --no-install-recommends \
      build-essential pkg-config git curl wget ca-certificates gnupg rsync sudo file xz-utils zstd python3 \
      meson ninja-build cmake xfce4-dev-tools valac itstool gettext desktop-file-utils appstream libglib2.0-dev-bin \
      live-build debootstrap squashfs-tools xorriso mtools dosfstools fakeroot cpio \
      syslinux syslinux-common isolinux grub-pc-bin grub-efi-amd64-bin grub-common util-linux

# --- Glaze build dependencies: compositor (smithay: wayland, xkbcommon, libinput, udev, seat, gbm/drm, EGL/GLES),
#     GTK4 shell and settings (gtk4, gtk4-layer-shell, glib, pango, cairo) ---
RUN apt-get install -y --no-install-recommends \
      libwayland-dev wayland-protocols libxkbcommon-dev libinput-dev libudev-dev libseat-dev libgbm-dev libdrm-dev \
      libegl-dev libgles-dev libegl1-mesa-dev libgl-dev libvulkan-dev libsystemd-dev libdbus-1-dev libxcb1-dev \
      libx11-dev libxcursor-dev libxi-dev libxrandr-dev libpixman-1-dev libfontconfig-dev libfreetype-dev \
      libgtk-4-dev libgtk4-layer-shell-dev libglib2.0-dev libpango1.0-dev libcairo2-dev libgdk-pixbuf-2.0-dev \
      libgraphene-1.0-dev libadwaita-1-dev clang libclang-dev libgtk-3-bin libgtk-4-bin

# --- Kiln apps: build dependencies of the Debian sources that are forked, plus the libs build-all.sh lists ---
RUN apt-get build-dep -y thunar foot gnome-text-editor gnome-calculator gnome-system-monitor loupe \
 && apt-get install -y --no-install-recommends \
      libgtk-3-dev libxfce4util-dev libxfce4ui-2-dev libexo-2-dev libxfconf-0-dev libgudev-1.0-dev libnotify-dev \
      libpango1.0-dev libgdk-pixbuf-2.0-dev libgirepository1.0-dev libxfce4panel-2.0-dev \
      libwayland-dev libxkbcommon-dev libfcft-dev libtllist-dev libutf8proc-dev scdoc foot-terminfo libutempter-dev \
      libadwaita-1-dev libgtksourceview-5-dev libgtop2-dev libgtkmm-4.0-dev libmpc-dev libmpfr-dev libxml2-dev \
      libgee-0.8-dev libeditorconfig-dev libenchant-2-dev libspelling-1-dev libicu-dev libpcre2-dev gobject-introspection \
      libpackagekit-glib2-dev libflatpak-dev \
 && rm -rf /var/lib/apt/lists/*

# --- Rust: Glaze needs >= 1.92 (Debian's rustc is older), so use rustup (stable) ---
# The crate registry (/opt/cargo/registry, /opt/cargo/git) is mounted as a volume by build-iso.sh to keep re-runs fast.
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable --no-modify-path \
 && cargo --version && rustc --version

WORKDIR /work
