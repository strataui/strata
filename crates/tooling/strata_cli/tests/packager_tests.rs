use std::fs;
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

    // 2. RPM (.rpm)
    let rpm_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::Rpm), &temp_dir).unwrap();
    assert_eq!(rpm_report.format, PackageFormat::Rpm);
    assert!(rpm_report.package_path.exists());
    let rpm_spec = temp_dir.join("dist/packages/strata-app.spec");
    assert!(rpm_spec.exists());
    let spec_text = fs::read_to_string(&rpm_spec).unwrap();
    assert!(spec_text.contains("Name:           strata-app"));

    // 3. AppImage (.AppImage)
    let appimage_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::AppImage), &temp_dir).unwrap();
    assert_eq!(appimage_report.format, PackageFormat::AppImage);
    assert!(appimage_report.package_path.exists());
    let apprun = temp_dir.join("dist/packages/strata-app.AppDir/AppRun");
    assert!(apprun.exists());
    let apprun_text = fs::read_to_string(&apprun).unwrap();
    assert!(apprun_text.contains("#!/bin/sh"));

    // 4. Arch PKGBUILD
    let arch_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::PkgBuild), &temp_dir).unwrap();
    assert_eq!(arch_report.format, PackageFormat::PkgBuild);
    assert!(arch_report.package_path.exists());
    let pkgbuild = temp_dir.join("dist/packages/PKGBUILD");
    assert!(pkgbuild.exists());
    let pkgbuild_text = fs::read_to_string(&pkgbuild).unwrap();
    assert!(pkgbuild_text.contains("pkgname=strata-app"));

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

    // 2. NSIS EXE (.exe)
    let exe_report = Packager::package(BuildTarget::Desktop, Some(PackageFormat::Exe), &temp_dir).unwrap();
    assert_eq!(exe_report.format, PackageFormat::Exe);
    assert!(exe_report.package_path.exists());
    let nsi_path = temp_dir.join("dist/packages/installer.nsi");
    assert!(nsi_path.exists());
    let nsi_text = fs::read_to_string(&nsi_path).unwrap();
    assert!(nsi_text.contains("!define PRODUCT_NAME \"Strata App\""));

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

    // 2. DMG (.dmg)
    let dmg_report = Packager::package(BuildTarget::MacOs, Some(PackageFormat::Dmg), &temp_dir).unwrap();
    assert_eq!(dmg_report.format, PackageFormat::Dmg);
    assert!(dmg_report.package_path.exists());

    // 3. PKG (.pkg)
    let pkg_report = Packager::package(BuildTarget::MacOs, Some(PackageFormat::Pkg), &temp_dir).unwrap();
    assert_eq!(pkg_report.format, PackageFormat::Pkg);
    assert!(pkg_report.package_path.exists());
    let dist_xml = temp_dir.join("dist/packages/Distribution.xml");
    assert!(dist_xml.exists());

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
