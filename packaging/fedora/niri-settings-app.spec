Name:           niri-settings-app
Version:        0.1.0
Release:        1%{?dist}
Summary:        Safe graphical settings app for the niri compositor

License:        GPL-3.0-or-later
URL:            https://github.com/pantarune/niri-settings
Source0:        %{url}/releases/download/v%{version}/%{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust-packaging
BuildRequires:  pkgconf-pkg-config
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel
BuildRequires:  desktop-file-utils
BuildRequires:  appstream
# The integration tests run `niri validate`.
BuildRequires:  niri

Requires:       gtk4
Requires:       libadwaita
Requires:       niri

%description
niri settings changes the niri config with a graphical interface. Every
change is checked with niri validate and can be undone. Comments and
formatting in the file stay intact.

The app also finds duplicate shortcuts, missing programs and broken
includes, imports shortcuts from Hyprland or Sway, and shows a live
picture for visual options.

%prep
%autosetup -n %{name}-%{version}

%build
%cargo_build

%check
%cargo_test
desktop-file-validate \
  data/io.github.niri_settings.NiriSettings.desktop
appstreamcli validate --no-net \
  data/io.github.niri_settings.NiriSettings.metainfo.xml

%install
%cargo_install
rm -f %{buildroot}%{_bindir}/niri-settings-schema-gen
install -Dm644 data/io.github.niri_settings.NiriSettings.desktop \
  %{buildroot}%{_datadir}/applications/io.github.niri_settings.NiriSettings.desktop
install -Dm644 data/io.github.niri_settings.NiriSettings.metainfo.xml \
  %{buildroot}%{_datadir}/metainfo/io.github.niri_settings.NiriSettings.metainfo.xml
install -Dm644 data/io.github.niri_settings.NiriSettings.svg \
  %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/io.github.niri_settings.NiriSettings.svg

%files
%license LICENSE
%{_bindir}/niri-settings-app
%{_bindir}/niri-settings-cli
%{_bindir}/niri-settings-daemon
%{_datadir}/applications/io.github.niri_settings.NiriSettings.desktop
%{_datadir}/metainfo/io.github.niri_settings.NiriSettings.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.niri_settings.NiriSettings.svg

%changelog
* Sat Nov 21 2026 pantarune <pantarune@users.noreply.github.com> - 0.1.0-1
- Initial package
