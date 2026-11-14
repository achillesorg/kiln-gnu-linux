# RPM spec for Glaze (FedoraDE).
#
# Builds the Rust crates at the repository root, the GTK4 apps in their own
# workspaces, and installs the OPT-IN Wayland session entry, themes and assets.
#
# This spec is authored on a non-Fedora host and has NOT been built with
# `rpmbuild`/`fedpkg` yet; package names (esp. gtk4-layer-shell) must be verified
# against the target Fedora release. It deliberately does NOT touch the login
# manager, the default session or the bootloader.

%global debug_package %{nil}
%global _description %{expand:
Glaze (FedoraDE) is a native Fedora Wayland desktop environment: a Rust core and
services, a GTK4 shell and settings app, and an optional managed-compositor
session. This package installs the Glaze binaries, the application registry
service, the opt-in `glaze-session` Wayland session entry, the Glaze Clay/Classic
themes and original assets. Installing it does not change your current desktop
session; see `%{_docdir}/%{name}/README.fedora`.}

Name:           glaze
Version:        0.1.0
Release:        1%{?dist}
Summary:        Glaze Desktop — a native Fedora Wayland desktop environment
License:        MIT OR Apache-2.0
URL:            https://example.invalid/glaze
# `Source0` is the release tarball; during local development the tree is used.
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust-packaging
BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gtk4-devel
BuildRequires:  gtk4-layer-shell-devel
BuildRequires:  systemd-rpm-macros
Requires:       gtk4
Requires:       gtk4-layer-shell
# `update-desktop-database` in %%post/%%postun (guarded, so it is optional at
# runtime but present on any normal Fedora Workstation).
Requires(post):   desktop-file-utils
Requires(postun): desktop-file-utils

%description
%{_description}

%prep
%autosetup -n %{name}-%{version}

%build
# Portable workspace (glaze-common/service/session/compositor/shell-core/backend).
%cargo_build --workspace --locked
# The GTK4 apps are separate workspaces (they need system GTK4 + layer-shell).
( cd apps/glaze-shell    && %{__cargo} build --release --locked )
( cd apps/glaze-settings && %{__cargo} build --release --locked )

%install
# Core binaries (the portable workspace also builds the `glaze` CLI).
install -Dpm0755 target/release/glaze-session     %{buildroot}%{_bindir}/glaze-session
install -Dpm0755 target/release/glaze-service     %{buildroot}%{_bindir}/glaze-service
install -Dpm0755 target/release/glaze-compositor  %{buildroot}%{_bindir}/glaze-compositor
install -Dpm0755 target/release/glaze             %{buildroot}%{_bindir}/glaze
install -Dpm0755 apps/glaze-shell/target/release/glaze-shell       %{buildroot}%{_bindir}/glaze-shell
install -Dpm0755 apps/glaze-settings/target/release/glaze-settings %{buildroot}%{_bindir}/glaze-settings

# OPT-IN Wayland session entry. It is only ever ADDED to wayland-sessions/; the
# display manager offers it as one more choice and removing the package removes
# it again (see packaging/fedora/README.md).
install -Dpm0644 packaging/fedora/glaze-session.desktop \
    %{buildroot}%{_datadir}/wayland-sessions/glaze.desktop

# An application entry so Glaze Settings appears in any app menu.
install -Dpm0644 packaging/fedora/glaze-settings.desktop \
    %{buildroot}%{_datadir}/applications/org.glaze.Settings.desktop

# Themes and assets (original/licensed only).
install -dpm0755 %{buildroot}%{_datadir}/glaze
cp -a themes/. %{buildroot}%{_datadir}/glaze/themes/ 2>/dev/null || :
cp -a assets/. %{buildroot}%{_datadir}/glaze/assets/ 2>/dev/null || :

# Man pages and shell completions, when the tree ships them.
if [ -d docs/man ]; then
    install -dpm0755 %{buildroot}%{_mandir}/man1
    for page in docs/man/*.1; do
        [ -e "$page" ] || continue
        install -pm0644 "$page" %{buildroot}%{_mandir}/man1/
    done
fi
if [ -d packaging/completions ]; then
    install -dpm0755 %{buildroot}%{_datadir}/bash-completion/completions
    install -dpm0755 %{buildroot}%{_datadir}/zsh/site-functions
    for c in packaging/completions/*.bash; do
        [ -e "$c" ] || continue
        install -pm0644 "$c" \
            "%{buildroot}%{_datadir}/bash-completion/completions/$(basename "$c" .bash)"
    done
    for c in packaging/completions/*.zsh; do
        [ -e "$c" ] || continue
        install -pm0644 "$c" \
            "%{buildroot}%{_datadir}/zsh/site-functions/_$(basename "$c" .zsh)"
    done
fi

# Documentation (includes the opt-in instructions and the user-install script
# reference).
install -Dpm0644 packaging/fedora/README.md \
    %{buildroot}%{_docdir}/%{name}/README.fedora

%post
# Refresh the desktop database so Glaze Settings is visible immediately.
[ -x /usr/bin/update-desktop-database ] && \
    /usr/bin/update-desktop-database -q %{_datadir}/applications &>/dev/null || :

%postun
[ -x /usr/bin/update-desktop-database ] && \
    /usr/bin/update-desktop-database -q %{_datadir}/applications &>/dev/null || :

%files
%license LICENSE-MIT LICENSE-APACHE
%doc README.md docs/
%{_bindir}/glaze-session
%{_bindir}/glaze-service
%{_bindir}/glaze-compositor
%{_bindir}/glaze
%{_bindir}/glaze-shell
%{_bindir}/glaze-settings
%{_datadir}/wayland-sessions/glaze.desktop
%{_datadir}/applications/org.glaze.Settings.desktop
%{_datadir}/glaze/themes
%{_datadir}/glaze/assets
%{_mandir}/man1/glaze*.1*
%{_datadir}/bash-completion/completions/glaze
%{_datadir}/bash-completion/completions/glaze-session
%{_datadir}/zsh/site-functions/_glaze
%{_datadir}/zsh/site-functions/_glaze-session
%{_docdir}/%{name}/README.fedora

%changelog
* Wed Nov 11 2026 Glaze contributors <fede@example.invalid> - 0.1.0-2
- Install the `glaze` CLI, man pages, shell completions and the Settings app
  entry; refresh the desktop database in %%post/%%postun.
* Wed Nov 11 2026 Glaze contributors <fede@example.invalid> - 0.1.0-1
- Initial packaging: core crate binaries, opt-in Wayland session, themes/assets.
