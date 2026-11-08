use strata_cli::prelude::*;

#[test]
fn test_subcommand_help_parsing() {
    // Top-level help
    assert_eq!(CliArgs::parse_from(["strata", "--help"]).unwrap(), CliCommand::Help(None));
    assert_eq!(CliArgs::parse_from(["strata", "-h"]).unwrap(), CliCommand::Help(None));
    assert_eq!(CliArgs::parse_from(["strata", "help"]).unwrap(), CliCommand::Help(None));

    // Subcommand help with --help
    assert_eq!(
        CliArgs::parse_from(["strata", "new", "--help"]).unwrap(),
        CliCommand::Help(Some("new".to_string()))
    );
    assert_eq!(
        CliArgs::parse_from(["strata", "dev", "--help"]).unwrap(),
        CliCommand::Help(Some("dev".to_string()))
    );
    assert_eq!(
        CliArgs::parse_from(["strata", "build", "--help"]).unwrap(),
        CliCommand::Help(Some("build".to_string()))
    );
    assert_eq!(
        CliArgs::parse_from(["strata", "package", "--help"]).unwrap(),
        CliCommand::Help(Some("package".to_string()))
    );
    assert_eq!(
        CliArgs::parse_from(["strata", "test", "--help"]).unwrap(),
        CliCommand::Help(Some("test".to_string()))
    );
    assert_eq!(
        CliArgs::parse_from(["strata", "sim", "--help"]).unwrap(),
        CliCommand::Help(Some("sim".to_string()))
    );

    // Subcommand help with -h
    assert_eq!(
        CliArgs::parse_from(["strata", "new", "-h"]).unwrap(),
        CliCommand::Help(Some("new".to_string()))
    );
    assert_eq!(
        CliArgs::parse_from(["strata", "dev", "-h"]).unwrap(),
        CliCommand::Help(Some("dev".to_string()))
    );
}

#[test]
fn test_rejection_of_invalid_options() {
    // Invalid options in 'new'
    let err = CliArgs::parse_from(["strata", "new", "--unknown-flag"]);
    assert!(err.is_err(), "Must reject unknown flag for 'new'");
    assert!(err.unwrap_err().contains("Unrecognized option"));

    // Invalid options in 'dev'
    let err = CliArgs::parse_from(["strata", "dev", "--invalid-dev-flag"]);
    assert!(err.is_err(), "Must reject unknown flag for 'dev'");
    assert!(err.unwrap_err().contains("Unrecognized option"));

    // Invalid options in 'build'
    let err = CliArgs::parse_from(["strata", "build", "--bogus"]);
    assert!(err.is_err(), "Must reject unknown flag for 'build'");

    // Invalid options in 'package'
    let err = CliArgs::parse_from(["strata", "package", "--bad-opt"]);
    assert!(err.is_err(), "Must reject unknown flag for 'package'");

    // Invalid options in 'test'
    let err = CliArgs::parse_from(["strata", "test", "--fake"]);
    assert!(err.is_err(), "Must reject unknown flag for 'test'");

    // Invalid options in 'sim'
    let err = CliArgs::parse_from(["strata", "sim", "--what"]);
    assert!(err.is_err(), "Must reject unknown flag for 'sim'");

    // Invalid top-level command
    let err = CliArgs::parse_from(["strata", "nonexistent-command"]);
    assert!(err.is_err(), "Must reject unknown subcommand");
}

#[test]
fn test_rejection_of_extraneous_positionals() {
    // 'new' takes at most 1 positional (name)
    let err = CliArgs::parse_from(["strata", "new", "name1", "name2"]);
    assert!(err.is_err(), "Must reject extra positional arguments for 'new'");
    assert!(err.unwrap_err().contains("Unexpected argument"));

    // 'dev' takes no positional arguments
    let err = CliArgs::parse_from(["strata", "dev", "extra-arg"]);
    assert!(err.is_err(), "Must reject positional arguments for 'dev'");

    // 'sim' takes no positional arguments
    let err = CliArgs::parse_from(["strata", "sim", "phone"]);
    assert!(err.is_err(), "Must reject positional arguments for 'sim'");
}

#[test]
fn test_package_formats_parsing() {
    // Debian
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "desktop", "--format", "deb"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::Deb)),
        _ => panic!("Expected Package command"),
    }

    // RPM
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "desktop", "--format", "rpm"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::Rpm)),
        _ => panic!("Expected Package command"),
    }

    // AppImage
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "desktop", "--format", "appimage"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::AppImage)),
        _ => panic!("Expected Package command"),
    }

    // PKGBUILD
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "desktop", "--format", "pkgbuild"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::PkgBuild)),
        _ => panic!("Expected Package command"),
    }

    // MSI
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "desktop", "--format", "msi"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::Msi)),
        _ => panic!("Expected Package command"),
    }

    // EXE
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "desktop", "--format", "exe"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::Exe)),
        _ => panic!("Expected Package command"),
    }

    // DMG
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "macos", "--format", "dmg"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::Dmg)),
        _ => panic!("Expected Package command"),
    }

    // PKG
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "macos", "--format", "pkg"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::Pkg)),
        _ => panic!("Expected Package command"),
    }

    // APP
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "macos", "--format", "app"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::App)),
        _ => panic!("Expected Package command"),
    }

    // Flatpak
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "linux", "--format", "flatpak"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::Flatpak)),
        _ => panic!("Expected Package command"),
    }

    // Snap
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "linux", "--format", "snap"]).unwrap();
    match cmd {
        CliCommand::Package { format, .. } => assert_eq!(format, Some(PackageFormat::Snap)),
        _ => panic!("Expected Package command"),
    }
}
