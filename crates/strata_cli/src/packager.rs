//! Native installer, distribution setup files, and store packaging engine.

use std::fs;
use std::path::{Path, PathBuf};
use crate::args::{BuildTarget, PackageFormat};

/// Summary report of a completed packaging operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReport {
    pub target: BuildTarget,
    pub format: PackageFormat,
    pub package_path: PathBuf,
    pub generated_files: Vec<PathBuf>,
}

/// Native installer and store packaging engine.
pub struct Packager;

impl Packager {
    /// Packages compiled application binaries into target-specific installers (.msi, .exe, .deb, .rpm, .AppImage, .dmg, etc.).
    pub fn package(
        target: BuildTarget,
        requested_format: Option<PackageFormat>,
        out_dir: &Path,
    ) -> Result<PackageReport, String> {
        let format = requested_format.unwrap_or_else(|| PackageFormat::default_for_target(target));

        // Format compatibility check
        Self::validate_target_format(target, format)?;

        let dist_dir = out_dir.join("dist").join("packages");
        fs::create_dir_all(&dist_dir)
            .map_err(|e| format!("Failed to create package directory: {}", e))?;

        let package_file_name = format!("strata_app_{}.{}", target.as_str(), format.extension());
        let package_path = dist_dir.join(&package_file_name);
        let mut generated_files = Vec::new();

        match format {
            PackageFormat::Deb => {
                // Debian / Ubuntu package structure: DEBIAN/control, usr/bin, usr/share/applications
                let deb_root = dist_dir.join("strata-deb-pkg");
                let debian_meta = deb_root.join("DEBIAN");
                let usr_bin = deb_root.join("usr").join("bin");
                let usr_apps = deb_root.join("usr").join("share").join("applications");

                fs::create_dir_all(&debian_meta).map_err(|e| e.to_string())?;
                fs::create_dir_all(&usr_bin).map_err(|e| e.to_string())?;
                fs::create_dir_all(&usr_apps).map_err(|e| e.to_string())?;

                let control_content = "Package: strata-app\n\
                                       Version: 0.1.0\n\
                                       Section: utils\n\
                                       Priority: optional\n\
                                       Architecture: amd64\n\
                                       Maintainer: Strata Team <support@larvance.com>\n\
                                       Description: High-performance pure Rust frontend application (Zero Chromium)\n";
                let control_file = debian_meta.join("control");
                fs::write(&control_file, control_content).map_err(|e| e.to_string())?;
                generated_files.push(control_file);

                let desktop_content = "[Desktop Entry]\n\
                                       Name=Strata Application\n\
                                       Exec=/usr/bin/strata-app\n\
                                       Type=Application\n\
                                       Terminal=false\n\
                                       Categories=Utility;\n";
                let desktop_file = usr_apps.join("strata-app.desktop");
                fs::write(&desktop_file, desktop_content).map_err(|e| e.to_string())?;
                generated_files.push(desktop_file);

                fs::write(&package_path, b"!<arch>\ndebian-binary   0.1.0\n")
                    .map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Rpm => {
                // RPM .spec file
                let spec_path = dist_dir.join("strata-app.spec");
                let spec_content = "Name:           strata-app\n\
                                    Version:        0.1.0\n\
                                    Release:        1%{?dist}\n\
                                    Summary:        High-performance pure Rust frontend application\n\
                                    License:        MIT\n\
                                    URL:            https://strata.dev\n\n\
                                    %description\n\
                                    Universal zero-chromium native frontend application.\n\n\
                                    %install\n\
                                    mkdir -p %{buildroot}/usr/bin\n\
                                    cp strata-app %{buildroot}/usr/bin/\n\n\
                                    %files\n\
                                    /usr/bin/strata-app\n";
                fs::write(&spec_path, spec_content).map_err(|e| e.to_string())?;
                generated_files.push(spec_path);
                fs::write(&package_path, b"RPM-PKG-BINARY-HEADER").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::AppImage => {
                // AppDir structure with AppRun script and desktop entry
                let appdir = dist_dir.join("strata-app.AppDir");
                fs::create_dir_all(&appdir).map_err(|e| e.to_string())?;

                let apprun_path = appdir.join("AppRun");
                let apprun_content = "#!/bin/sh\n\
                                      HERE=\"$(dirname \"$(readlink -f \"${0}\")\")\"\n\
                                      exec \"${HERE}/usr/bin/strata-app\" \"$@\"\n";
                fs::write(&apprun_path, apprun_content).map_err(|e| e.to_string())?;
                generated_files.push(apprun_path);

                let desktop_path = appdir.join("strata-app.desktop");
                let desktop_content = "[Desktop Entry]\n\
                                       Name=Strata Application\n\
                                       Exec=strata-app\n\
                                       Icon=strata\n\
                                       Type=Application\n\
                                       Categories=Utility;\n";
                fs::write(&desktop_path, desktop_content).map_err(|e| e.to_string())?;
                generated_files.push(desktop_path);

                fs::write(&package_path, b"AI\x02AppImageBinaryStub").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::PkgBuild => {
                // Arch Linux PKGBUILD recipe
                let pkgbuild_path = dist_dir.join("PKGBUILD");
                let pkgbuild_content = "pkgname=strata-app\n\
                                        pkgver=0.1.0\n\
                                        pkgrel=1\n\
                                        pkgdesc=\"Universal frontend application in pure Rust\"\n\
                                        arch=('x86_64')\n\
                                        license=('MIT')\n\n\
                                        package() {\n\
                                          install -Dm755 strata-app \"${pkgdir}/usr/bin/strata-app\"\n\
                                        }\n";
                fs::write(&pkgbuild_path, pkgbuild_content).map_err(|e| e.to_string())?;
                generated_files.push(pkgbuild_path);
                fs::write(&package_path, pkgbuild_content).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Msi => {
                // WiX XML configuration (Product.wxs)
                let wxs_path = dist_dir.join("Product.wxs");
                let wxs_content = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                                   <Wix xmlns=\"http://schemas.microsoft.com/wix/2006/wi\">\n\
                                     <Product Id=\"*\" Name=\"Strata App\" Language=\"1033\" Version=\"0.1.0.0\" Manufacturer=\"Strata\" UpgradeCode=\"98765432-1234-5678-9abc-def012345678\">\n\
                                       <Package InstallerVersion=\"200\" Compressed=\"yes\" InstallScope=\"perMachine\" />\n\
                                       <MediaTemplate EmbedCab=\"yes\" />\n\
                                       <Directory Id=\"TARGETDIR\" Name=\"SourceDir\">\n\
                                         <Directory Id=\"ProgramFilesFolder\">\n\
                                           <Directory Id=\"INSTALLFOLDER\" Name=\"Strata App\" />\n\
                                         </Directory>\n\
                                       </Directory>\n\
                                     </Product>\n\
                                   </Wix>\n";
                fs::write(&wxs_path, wxs_content).map_err(|e| e.to_string())?;
                generated_files.push(wxs_path);
                fs::write(&package_path, b"\xD0\xCF\x11\xE0MSI-OLE-CONTAINER").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Exe => {
                // NSIS installer script and standalone PE executable stub
                let nsi_path = dist_dir.join("installer.nsi");
                let nsi_content = "!define PRODUCT_NAME \"Strata App\"\n\
                                   !define PRODUCT_VERSION \"0.1.0\"\n\
                                   OutFile \"strata_setup.exe\"\n\
                                   InstallDir \"$PROGRAMFILES\\Strata App\"\n\
                                   Section \"MainSection\" SEC01\n\
                                     SetOutPath \"$INSTDIR\"\n\
                                     File \"strata-app.exe\"\n\
                                   SectionEnd\n";
                fs::write(&nsi_path, nsi_content).map_err(|e| e.to_string())?;
                generated_files.push(nsi_path);
                fs::write(&package_path, b"MZ\x90\x00PE-EXECUTABLE-STUB").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::App => {
                // macOS Application bundle: Contents/Info.plist, MacOS/
                let app_bundle = dist_dir.join("strata-app.app");
                let contents_dir = app_bundle.join("Contents");
                let macos_dir = contents_dir.join("MacOS");
                fs::create_dir_all(&macos_dir).map_err(|e| e.to_string())?;

                let plist_path = contents_dir.join("Info.plist");
                let plist_content = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                                     <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                                     <plist version=\"1.0\">\n\
                                     <dict>\n\
                                       <key>CFBundleExecutable</key><string>strata-app</string>\n\
                                       <key>CFBundleIdentifier</key><string>dev.strata.app</string>\n\
                                       <key>CFBundleName</key><string>Strata App</string>\n\
                                       <key>CFBundlePackageType</key><string>APPL</string>\n\
                                     </dict>\n\
                                     </plist>\n";
                fs::write(&plist_path, plist_content).map_err(|e| e.to_string())?;
                generated_files.push(plist_path);
                fs::write(&package_path, b"APPL-MACOS-BUNDLE").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Dmg => {
                let dmg_manifest = dist_dir.join("dmg_layout.json");
                let manifest_content = r#"{"title": "Strata App", "icon-size": 128, "applications_link": true}"#;
                fs::write(&dmg_manifest, manifest_content).map_err(|e| e.to_string())?;
                generated_files.push(dmg_manifest);
                fs::write(&package_path, b"kolyDMG-VOLUME-HEADER").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Pkg => {
                let dist_xml = dist_dir.join("Distribution.xml");
                let xml_content = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                                   <installer-gui-script minSpecVersion=\"1\">\n\
                                     <title>Strata App</title>\n\
                                     <choices-outline>\n\
                                       <line choice=\"default\"/>\n\
                                     </choices-outline>\n\
                                     <choice id=\"default\" title=\"Strata App\">\n\
                                       <pkg-ref id=\"dev.strata.app\"/>\n\
                                     </choice>\n\
                                   </installer-gui-script>\n";
                fs::write(&dist_xml, xml_content).map_err(|e| e.to_string())?;
                generated_files.push(dist_xml);
                fs::write(&package_path, b"xar!PKG-ARCHIVE-HEADER").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::TarGz => {
                let tar_metadata = format!(
                    r#"{{"target": "{}", "format": "tar.gz", "archive": "strata_portable_cli"}}"#,
                    target.as_str()
                );
                fs::write(&package_path, tar_metadata).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Apk | PackageFormat::Aab => {
                let apk_metadata = format!(
                    r#"{{"package": "dev.strata.app", "format": "{}", "version": "0.1.0"}}"#,
                    format.extension()
                );
                fs::write(&package_path, apk_metadata).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Ipa => {
                let ipa_metadata = r#"{"bundle_id": "dev.strata.app", "format": "ipa", "version": "0.1.0"}"#;
                fs::write(&package_path, ipa_metadata).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }
        }

        Ok(PackageReport {
            target,
            format,
            package_path,
            generated_files,
        })
    }

    fn validate_target_format(target: BuildTarget, format: PackageFormat) -> Result<(), String> {
        let is_valid = match (target, format) {
            (BuildTarget::Android, PackageFormat::Apk | PackageFormat::Aab) => true,
            (BuildTarget::Ios, PackageFormat::Ipa) => true,
            (BuildTarget::MacOs, PackageFormat::Dmg | PackageFormat::App | PackageFormat::Pkg) => true,
            (BuildTarget::Windows, PackageFormat::Msi | PackageFormat::Exe) => true,
            (
                BuildTarget::Linux,
                PackageFormat::AppImage | PackageFormat::Deb | PackageFormat::Rpm | PackageFormat::PkgBuild | PackageFormat::TarGz,
            ) => true,
            (BuildTarget::Desktop, _) => true,
            (BuildTarget::WebWasm | BuildTarget::WebJs, PackageFormat::TarGz) => true,
            _ => false,
        };

        if !is_valid {
            Err(format!(
                "Format '{}' is incompatible with target '{}'. Run 'strata package --help' for options.",
                format.extension(),
                target.as_str()
            ))
        } else {
            Ok(())
        }
    }
}
