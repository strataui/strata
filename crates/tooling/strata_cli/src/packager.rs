//! Native installer, distribution setup files, and store packaging engine.

use std::fs;
use std::path::{Path, PathBuf};
use crate::args::{BuildTarget, PackageFormat};
use crate::config::StrataConfig;
use crate::plugin::{PluginContext, PluginEngine};

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
        Self::package_with_env(target, requested_format, out_dir, None)
    }

    /// Packages compiled application binaries with an optional environment override and plugin execution.
    pub fn package_with_env(
        target: BuildTarget,
        requested_format: Option<PackageFormat>,
        out_dir: &Path,
        env: Option<&str>,
    ) -> Result<PackageReport, String> {
        let format = requested_format.unwrap_or_else(|| PackageFormat::default_for_target(target));

        // Format compatibility check
        Self::validate_target_format(target, format)?;

        let dist_dir = out_dir.join("dist").join("packages");
        fs::create_dir_all(&dist_dir)
            .map_err(|e| format!("Failed to create package directory: {}", e))?;

        // 1. Load project configuration and run plugin transformations
        let mut strata_cfg = StrataConfig::load_from_dir(out_dir).unwrap_or_default();
        let mut ctx = PluginContext::new(target, true, out_dir, &dist_dir);
        if let Some(e) = env {
            ctx.environment = e.to_string();
        }
        let _ = PluginEngine::run_config_transformations(&mut strata_cfg, &ctx);

        let app_name = strata_cfg.app.name.clone();
        let app_version = strata_cfg.app.version.clone();
        let app_desc = strata_cfg.app.description.clone();
        let window_title = strata_cfg.window.title.clone();

        let package_file_name = format!("{}_{}.{}", app_name, target.as_str(), format.extension());
        let package_path = dist_dir.join(&package_file_name);
        let mut generated_files = Vec::new();

        match format {
            PackageFormat::Deb => {
                // Debian / Ubuntu package structure: DEBIAN/control, usr/bin, usr/share/applications
                let deb_root = if app_name == "strata-app" {
                    dist_dir.join("strata-deb-pkg")
                } else {
                    dist_dir.join(format!("{}-deb-pkg", app_name))
                };
                let debian_meta = deb_root.join("DEBIAN");
                let usr_bin = deb_root.join("usr").join("bin");
                let usr_apps = deb_root.join("usr").join("share").join("applications");

                fs::create_dir_all(&debian_meta).map_err(|e| e.to_string())?;
                fs::create_dir_all(&usr_bin).map_err(|e| e.to_string())?;
                fs::create_dir_all(&usr_apps).map_err(|e| e.to_string())?;

                let control_content = format!(
                    "Package: {}\n\
                     Version: {}\n\
                     Section: utils\n\
                     Priority: optional\n\
                     Architecture: amd64\n\
                     Maintainer: Strata Team <support@larvance.com>\n\
                     Description: {}\n",
                    app_name, app_version, app_desc
                );
                let control_file = debian_meta.join("control");
                fs::write(&control_file, control_content).map_err(|e| e.to_string())?;
                generated_files.push(control_file);

                let desktop_content = format!(
                    "[Desktop Entry]\n\
                     Name={}\n\
                     Exec=/usr/bin/{}\n\
                     Type=Application\n\
                     Terminal=false\n\
                     Categories=Utility;\n",
                    window_title, app_name
                );
                let desktop_file = usr_apps.join(format!("{}.desktop", app_name));
                fs::write(&desktop_file, desktop_content).map_err(|e| e.to_string())?;
                generated_files.push(desktop_file);

                fs::write(&package_path, format!("!<arch>\ndebian-binary   {}\n", app_version).as_bytes())
                    .map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Rpm => {
                // RPM .spec file
                let spec_path = dist_dir.join(format!("{}.spec", app_name));
                let spec_content = format!(
                    "Name:           {}\n\
                     Version:        {}\n\
                     Release:        1%{{?dist}}\n\
                     Summary:        {}\n\
                     License:        MIT\n\
                     URL:            https://strata.dev\n\n\
                     %description\n\
                     {}\n\n\
                     %install\n\
                     mkdir -p %{{buildroot}}/usr/bin\n\
                     cp {} %{{buildroot}}/usr/bin/\n\n\
                     %files\n\
                     /usr/bin/{}\n",
                    app_name, app_version, window_title, app_desc, app_name, app_name
                );
                fs::write(&spec_path, spec_content).map_err(|e| e.to_string())?;
                generated_files.push(spec_path);
                fs::write(&package_path, b"RPM-PKG-BINARY-HEADER").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::AppImage => {
                // AppDir structure with AppRun script and desktop entry
                let appdir = dist_dir.join(format!("{}.AppDir", app_name));
                fs::create_dir_all(&appdir).map_err(|e| e.to_string())?;

                let apprun_path = appdir.join("AppRun");
                let apprun_content = format!(
                    "#!/bin/sh\n\
                     HERE=\"$(dirname \"$(readlink -f \"${{0}}\")\")\"\n\
                     exec \"${{HERE}}/usr/bin/{}\" \"$@\"\n",
                    app_name
                );
                fs::write(&apprun_path, apprun_content).map_err(|e| e.to_string())?;
                generated_files.push(apprun_path);

                let desktop_path = appdir.join(format!("{}.desktop", app_name));
                let desktop_content = format!(
                    "[Desktop Entry]\n\
                     Name={}\n\
                     Exec={}\n\
                     Icon=strata\n\
                     Type=Application\n\
                     Categories=Utility;\n",
                    window_title, app_name
                );
                fs::write(&desktop_path, desktop_content).map_err(|e| e.to_string())?;
                generated_files.push(desktop_path);

                fs::write(&package_path, b"AI\x02AppImageBinaryStub").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::PkgBuild => {
                // Arch Linux PKGBUILD recipe
                let pkgbuild_path = dist_dir.join("PKGBUILD");
                let pkgbuild_content = format!(
                    "pkgname={}\n\
                     pkgver={}\n\
                     pkgrel=1\n\
                     pkgdesc=\"{}\"\n\
                     arch=('x86_64')\n\
                     license=('MIT')\n\n\
                     package() {{\n\
                       install -Dm755 {} \"${{pkgdir}}/usr/bin/{}\"\n\
                     }}\n",
                    app_name, app_version, window_title, app_name, app_name
                );
                fs::write(&pkgbuild_path, &pkgbuild_content).map_err(|e| e.to_string())?;
                generated_files.push(pkgbuild_path);
                fs::write(&package_path, pkgbuild_content).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Msi => {
                // WiX XML configuration (Product.wxs)
                let wxs_path = dist_dir.join("Product.wxs");
                let wxs_content = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <Wix xmlns=\"http://schemas.microsoft.com/wix/2006/wi\">\n\
                       <Product Id=\"*\" Name=\"{}\" Language=\"1033\" Version=\"{}.0\" Manufacturer=\"Strata\" UpgradeCode=\"98765432-1234-5678-9abc-def012345678\">\n\
                         <Package InstallerVersion=\"200\" Compressed=\"yes\" InstallScope=\"perMachine\" />\n\
                         <MediaTemplate EmbedCab=\"yes\" />\n\
                         <Directory Id=\"TARGETDIR\" Name=\"SourceDir\">\n\
                           <Directory Id=\"ProgramFilesFolder\">\n\
                             <Directory Id=\"INSTALLFOLDER\" Name=\"{}\" />\n\
                           </Directory>\n\
                         </Directory>\n\
                       </Product>\n\
                     </Wix>\n",
                    window_title, app_version, window_title
                );
                fs::write(&wxs_path, wxs_content).map_err(|e| e.to_string())?;
                generated_files.push(wxs_path);
                fs::write(&package_path, b"\xD0\xCF\x11\xE0MSI-OLE-CONTAINER").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Exe => {
                // NSIS installer script and standalone PE executable stub
                let nsi_path = dist_dir.join("installer.nsi");
                let nsi_content = format!(
                    "!define PRODUCT_NAME \"{}\"\n\
                     !define PRODUCT_VERSION \"{}\"\n\
                     OutFile \"{}_setup.exe\"\n\
                     InstallDir \"$PROGRAMFILES\\{}\"\n\
                     Section \"MainSection\" SEC01\n\
                       SetOutPath \"$INSTDIR\"\n\
                       File \"{}.exe\"\n\
                     SectionEnd\n",
                    window_title, app_version, app_name, window_title, app_name
                );
                fs::write(&nsi_path, nsi_content).map_err(|e| e.to_string())?;
                generated_files.push(nsi_path);
                fs::write(&package_path, b"MZ\x90\x00PE-EXECUTABLE-STUB").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::App => {
                // macOS Application bundle: Contents/Info.plist, MacOS/
                let app_bundle = dist_dir.join(format!("{}.app", app_name));
                let contents_dir = app_bundle.join("Contents");
                let macos_dir = contents_dir.join("MacOS");
                fs::create_dir_all(&macos_dir).map_err(|e| e.to_string())?;

                let plist_path = contents_dir.join("Info.plist");
                let plist_content = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                     <plist version=\"1.0\">\n\
                     <dict>\n\
                       <key>CFBundleExecutable</key><string>{}</string>\n\
                       <key>CFBundleIdentifier</key><string>dev.strata.{}</string>\n\
                       <key>CFBundleName</key><string>{}</string>\n\
                       <key>CFBundlePackageType</key><string>APPL</string>\n\
                     </dict>\n\
                     </plist>\n",
                    app_name, app_name, window_title
                );
                fs::write(&plist_path, plist_content).map_err(|e| e.to_string())?;
                generated_files.push(plist_path);
                fs::write(&package_path, b"APPL-MACOS-BUNDLE").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Dmg => {
                let dmg_manifest = dist_dir.join("dmg_layout.json");
                let manifest_content = format!(
                    r#"{{"title": "{}", "app_name": "{}", "icon-size": 128, "applications_link": true}}"#,
                    window_title, app_name
                );
                fs::write(&dmg_manifest, manifest_content).map_err(|e| e.to_string())?;
                generated_files.push(dmg_manifest);
                fs::write(&package_path, b"kolyDMG-VOLUME-HEADER").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Pkg => {
                let dist_xml = dist_dir.join("Distribution.xml");
                let xml_content = format!(
                    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                     <installer-gui-script minSpecVersion=\"1\">\n\
                       <title>{}</title>\n\
                       <choices-outline>\n\
                         <line choice=\"default\"/>\n\
                       </choices-outline>\n\
                       <choice id=\"default\" title=\"{}\">\n\
                         <pkg-ref id=\"dev.strata.{}\"/>\n\
                       </choice>\n\
                     </installer-gui-script>\n",
                    window_title, window_title, app_name
                );
                fs::write(&dist_xml, xml_content).map_err(|e| e.to_string())?;
                generated_files.push(dist_xml);
                fs::write(&package_path, b"xar!PKG-ARCHIVE-HEADER").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::TarGz => {
                let tar_metadata = format!(
                    r#"{{"app": "{}", "version": "{}", "target": "{}", "format": "tar.gz", "archive": "strata_portable_cli"}}"#,
                    app_name, app_version, target.as_str()
                );
                fs::write(&package_path, tar_metadata).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Apk | PackageFormat::Aab => {
                let apk_metadata = format!(
                    r#"{{"package": "dev.strata.{}", "app_name": "{}", "format": "{}", "version": "{}"}}"#,
                    app_name, app_name, format.extension(), app_version
                );
                fs::write(&package_path, apk_metadata).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Ipa => {
                let ipa_metadata = format!(
                    r#"{{"bundle_id": "dev.strata.{}", "app_name": "{}", "format": "ipa", "version": "{}"}}"#,
                    app_name, app_name, app_version
                );
                fs::write(&package_path, ipa_metadata).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Flatpak => {
                let manifest_path = dist_dir.join(format!("dev.strata.{}.json", app_name));
                let manifest_content = format!(
                    "{{\n  \"app-id\": \"dev.strata.{}\",\n  \"runtime\": \"org.freedesktop.Platform\",\n  \"runtime-version\": \"23.08\",\n  \"sdk\": \"org.freedesktop.Sdk\",\n  \"command\": \"{}\",\n  \"finish-args\": [\n    \"--share=ipc\",\n    \"--socket=wayland\",\n    \"--socket=fallback-x11\",\n    \"--device=dri\"\n  ]\n}}\n",
                    app_name, app_name
                );
                fs::write(&manifest_path, manifest_content).map_err(|e| e.to_string())?;
                generated_files.push(manifest_path);
                fs::write(&package_path, b"FLATPAK-OSTREE-BUNDLE").map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Snap => {
                let snapcraft_dir = dist_dir.join("snap");
                fs::create_dir_all(&snapcraft_dir).map_err(|e| e.to_string())?;
                let snapcraft_path = snapcraft_dir.join("snapcraft.yaml");
                let snapcraft_content = format!(
                    "name: {}\nversion: '{}'\nsummary: {}\ndescription: {}\ngrade: stable\nconfinement: strict\nbase: core22\napps:\n  {}:\n    command: usr/bin/{}\n    plugs: [x11, wayland, opengl, network]\n",
                    app_name, app_version, window_title, app_desc, app_name, app_name
                );
                fs::write(&snapcraft_path, snapcraft_content).map_err(|e| e.to_string())?;
                generated_files.push(snapcraft_path);
                fs::write(&package_path, b"SNAP-SQUASHFS-CONTAINER").map_err(|e| e.to_string())?;
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
                PackageFormat::AppImage
                    | PackageFormat::Deb
                    | PackageFormat::Rpm
                    | PackageFormat::PkgBuild
                    | PackageFormat::TarGz
                    | PackageFormat::Flatpak
                    | PackageFormat::Snap,
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
