use std::fs;
use std::io::Read;
use strata_cli::prelude::*;

#[test]
fn test_package_linux_distributions() {
    let temp_dir = std::env::temp_dir().join(format!("strata_pkg_linux_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. Debian (.deb)
    let deb_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::Deb), &temp_dir).unwrap();
    assert_eq!(deb_report.format, PackageFormat::Deb);
    assert!(deb_report.package_path.exists());
    let deb_control = temp_dir.join("dist/packages/strata-deb-pkg/DEBIAN/control");
    assert!(deb_control.exists());
    let control_text = fs::read_to_string(&deb_control).unwrap();
    assert!(control_text.contains("Package: strata-app"));
    assert!(control_text.contains("Architecture: amd64"));

    let deb_bytes = fs::read(&deb_report.package_path).unwrap();
    assert!(deb_bytes.starts_with(b"!<arch>\n"), "Debian package must start with ar magic !<arch>\\n");
    assert!(deb_bytes.len() > 500, "Genuine .deb package must contain control and data tarballs");
    let deb_str = String::from_utf8_lossy(&deb_bytes);
    assert!(deb_str.contains("debian-binary"), "Must contain debian-binary member");
    assert!(deb_str.contains("control.tar.gz"), "Must contain control.tar.gz member");
    assert!(deb_str.contains("data.tar.gz"), "Must contain data.tar.gz member");

    // 2. RPM (.rpm)
    let rpm_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::Rpm), &temp_dir).unwrap();
    assert_eq!(rpm_report.format, PackageFormat::Rpm);
    assert!(rpm_report.package_path.exists());
    let rpm_spec = temp_dir.join("dist/packages/strata-app.spec");
    assert!(rpm_spec.exists());
    let spec_text = fs::read_to_string(&rpm_spec).unwrap();
    assert!(spec_text.contains("Name:           strata-app"));
    let rpm_bytes = fs::read(&rpm_report.package_path).unwrap();
    assert_eq!(&rpm_bytes[..4], &[0xED, 0xAB, 0xEE, 0xDB], "RPM package must start with authentic 0xEDAB EEDB magic lead");
    assert!(rpm_bytes.len() > 200, "RPM package must contain 96-byte lead, headers, and payload");

    // 3. AppImage (.AppImage)
    let appimage_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::AppImage), &temp_dir).unwrap();
    assert_eq!(appimage_report.format, PackageFormat::AppImage);
    assert!(appimage_report.package_path.exists());
    let apprun = temp_dir.join("dist/packages/strata-app.AppDir/AppRun");
    assert!(apprun.exists());
    let apprun_text = fs::read_to_string(&apprun).unwrap();
    assert!(apprun_text.contains("#!/bin/sh"));
    let appimage_bin = temp_dir.join("dist/packages/strata-app.AppDir/usr/bin/strata-app");
    assert!(appimage_bin.exists(), "AppDir/usr/bin/<bin> must exist");
    let appimage_bytes = fs::read(&appimage_report.package_path).unwrap();
    assert!(appimage_bytes.starts_with(b"\x7fELF"), "AppImage must have ELF binary header");
    assert_eq!(&appimage_bytes[8..11], b"AI\x02", "AppImage must contain Type 2 AI signature");

    // 4. Arch PKGBUILD
    let arch_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::PkgBuild), &temp_dir).unwrap();
    assert_eq!(arch_report.format, PackageFormat::PkgBuild);
    assert!(arch_report.package_path.exists());
    let pkgbuild = temp_dir.join("dist/packages/PKGBUILD");
    assert!(pkgbuild.exists());
    let pkgbuild_text = fs::read_to_string(&pkgbuild).unwrap();
    assert!(pkgbuild_text.contains("pkgname=strata-app"));

    // 5. Flatpak (.flatpak)
    let flatpak_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::Flatpak), &temp_dir).unwrap();
    assert_eq!(flatpak_report.format, PackageFormat::Flatpak);
    assert!(flatpak_report.package_path.exists());
    let flatpak_manifest = temp_dir.join("dist/packages/dev.strata.strata-app.json");
    assert!(flatpak_manifest.exists());
    let manifest_text = fs::read_to_string(&flatpak_manifest).unwrap();
    assert!(manifest_text.contains("org.freedesktop.Platform"));
    let flatpak_bytes = fs::read(&flatpak_report.package_path).unwrap();
    assert_eq!(&flatpak_bytes[0..2], &[0x1F, 0x8B], "Flatpak bundle container must be a valid gzip archive");

    // 6. Snap (.snap)
    let snap_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::Snap), &temp_dir).unwrap();
    assert_eq!(snap_report.format, PackageFormat::Snap);
    assert!(snap_report.package_path.exists());
    let snapcraft_yaml = temp_dir.join("dist/packages/snap/snapcraft.yaml");
    assert!(snapcraft_yaml.exists());
    let snapcraft_text = fs::read_to_string(&snapcraft_yaml).unwrap();
    assert!(snapcraft_text.contains("confinement: strict"));
    let snap_bytes = fs::read(&snap_report.package_path).unwrap();
    assert_eq!(&snap_bytes[0..4], b"hsqs", "Snap package must be a valid SquashFS container");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_package_windows_distributions() {
    let temp_dir = std::env::temp_dir().join(format!("strata_pkg_win_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. WiX MSI (.msi)
    let msi_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::Msi), &temp_dir).unwrap();
    assert_eq!(msi_report.format, PackageFormat::Msi);
    assert!(msi_report.package_path.exists());
    let wxs_path = temp_dir.join("dist/packages/Product.wxs");
    assert!(wxs_path.exists());
    let wxs_text = fs::read_to_string(&wxs_path).unwrap();
    assert!(wxs_text.contains("<Wix"));
    assert!(wxs_text.contains("Strata App"));

    let msi_bytes = fs::read(&msi_report.package_path).unwrap();
    assert_eq!(
        &msi_bytes[0..8],
        &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1],
        "MSI package must be an authentic Compound File Binary (OLE2) container"
    );
    assert_eq!(msi_bytes.len() % 512, 0, "MSI sector size must be a multiple of 512 bytes");
    assert!(msi_bytes.len() >= 2048, "MSI file must contain header, directory, FAT, and streams");

    // 2. NSIS EXE (.exe)
    let exe_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::Exe), &temp_dir).unwrap();
    assert_eq!(exe_report.format, PackageFormat::Exe);
    assert!(exe_report.package_path.exists());
    let nsi_path = temp_dir.join("dist/packages/installer.nsi");
    assert!(nsi_path.exists());
    let nsi_text = fs::read_to_string(&nsi_path).unwrap();
    assert!(nsi_text.contains("!define PRODUCT_NAME \"Strata App\""));

    let exe_bytes = fs::read(&exe_report.package_path).unwrap();
    assert_eq!(&exe_bytes[0..2], b"MZ", "Windows installer must begin with DOS MZ header");
    let pe_offset = u32::from_le_bytes(exe_bytes[60..64].try_into().unwrap()) as usize;
    assert_eq!(&exe_bytes[pe_offset..pe_offset + 4], b"PE\0\0", "PE header signature must exist at e_lfanew");
    assert!(exe_bytes.len() >= 2048, "Windows setup executable must be a valid PE32+ image");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_package_macos_distributions() {
    let temp_dir = std::env::temp_dir().join(format!("strata_pkg_mac_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. App Bundle (.app)
    let app_report = Packager::package(BuildTarget::MacOs, Some(PackageFormat::App), &temp_dir).unwrap();
    assert_eq!(app_report.format, PackageFormat::App);
    let plist_path = temp_dir.join("dist/packages/strata-app.app/Contents/Info.plist");
    assert!(plist_path.exists());
    let plist_text = fs::read_to_string(&plist_path).unwrap();
    assert!(plist_text.contains("CFBundleExecutable"));

    let exec_path = temp_dir.join("dist/packages/strata-app.app/Contents/MacOS/strata-app");
    assert!(exec_path.exists(), "Contents/MacOS/<bin> must exist");
    let pkginfo_path = temp_dir.join("dist/packages/strata-app.app/Contents/PkgInfo");
    assert!(pkginfo_path.exists());
    assert_eq!(fs::read_to_string(&pkginfo_path).unwrap(), "APPL????");

    // 2. DMG (.dmg)
    let dmg_report = Packager::package(BuildTarget::MacOs, Some(PackageFormat::Dmg), &temp_dir).unwrap();
    assert_eq!(dmg_report.format, PackageFormat::Dmg);
    assert!(dmg_report.package_path.exists());
    let dmg_bytes = fs::read(&dmg_report.package_path).unwrap();
    assert!(dmg_bytes.len() >= 1024, "DMG disk image must contain payload and UDIF trailer");
    let trailer_start = dmg_bytes.len() - 512;
    assert_eq!(&dmg_bytes[trailer_start..trailer_start + 4], b"koly", "DMG must end with 512-byte UDIF koly trailer");

    // 3. PKG (.pkg)
    let pkg_report = Packager::package(BuildTarget::MacOs, Some(PackageFormat::Pkg), &temp_dir).unwrap();
    assert_eq!(pkg_report.format, PackageFormat::Pkg);
    assert!(pkg_report.package_path.exists());
    let dist_xml = temp_dir.join("dist/packages/Distribution.xml");
    assert!(dist_xml.exists());
    let pkg_bytes = fs::read(&pkg_report.package_path).unwrap();
    assert_eq!(&pkg_bytes[0..4], b"xar!", "macOS PKG installer must be a valid XAR archive");
    assert!(pkg_bytes.len() >= 256, "PKG archive must contain header and table of contents");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_package_universal_tar_gz() {
    let temp_dir = std::env::temp_dir().join(format!("strata_pkg_tar_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let tar_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::TarGz), &temp_dir).unwrap();
    assert_eq!(tar_report.format, PackageFormat::TarGz);
    assert!(tar_report.package_path.exists());

    let tar_gz_bytes = fs::read(&tar_report.package_path).unwrap();
    assert_eq!(&tar_gz_bytes[0..2], &[0x1F, 0x8B], "tar.gz must start with gzip magic [0x1F, 0x8B]");

    // Verify gzip archive decompresses into a valid tar archive
    let mut decoder = flate2::read::GzDecoder::new(&tar_gz_bytes[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).expect("GzDecoder must decompress payload");
    assert!(decompressed.len() >= 1024, "Decompressed tar archive must contain tar headers");
    let tar_str = String::from_utf8_lossy(&decompressed);
    assert!(tar_str.contains("usr/bin/strata-app"), "Tar archive must contain app binary entry");
    assert!(tar_str.contains("strata.json"), "Tar archive must contain application manifest");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_package_mobile_distributions() {
    let temp_dir = std::env::temp_dir().join(format!("strata_pkg_mobile_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. Android APK (.apk)
    let apk_report = Packager::package(BuildTarget::Android, Some(PackageFormat::Apk), &temp_dir).unwrap();
    assert_eq!(apk_report.format, PackageFormat::Apk);
    assert!(apk_report.package_path.exists());
    let apk_bytes = fs::read(&apk_report.package_path).unwrap();
    assert_eq!(&apk_bytes[0..4], &[0x50, 0x4B, 0x03, 0x04], "APK must be a valid ZIP archive");
    let apk_str = String::from_utf8_lossy(&apk_bytes);
    assert!(apk_str.contains("AndroidManifest.xml"), "APK must contain AndroidManifest.xml");
    assert!(apk_str.contains("lib/arm64-v8a/libstrata-app.so"), "APK must contain native library");

    // 2. iOS IPA (.ipa)
    let ipa_report = Packager::package(BuildTarget::Ios, Some(PackageFormat::Ipa), &temp_dir).unwrap();
    assert_eq!(ipa_report.format, PackageFormat::Ipa);
    assert!(ipa_report.package_path.exists());
    let ipa_bytes = fs::read(&ipa_report.package_path).unwrap();
    assert_eq!(&ipa_bytes[0..4], &[0x50, 0x4B, 0x03, 0x04], "IPA must be a valid ZIP archive");
    let ipa_str = String::from_utf8_lossy(&ipa_bytes);
    assert!(ipa_str.contains("Payload/strata-app.app/Info.plist"), "IPA must contain Payload app bundle");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_packager_incompatible_format_rejection() {
    let temp_dir = std::env::temp_dir().join(format!("strata_pkg_incompat_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // Cannot package Android target as .deb or .msi
    assert!(Packager::package(BuildTarget::Android, Some(PackageFormat::Deb), &temp_dir).is_err());
    assert!(Packager::package(BuildTarget::Android, Some(PackageFormat::Msi), &temp_dir).is_err());
    assert!(Packager::package(BuildTarget::WebWasm, Some(PackageFormat::Exe), &temp_dir).is_err());

    let _ = fs::remove_dir_all(&temp_dir);
}
