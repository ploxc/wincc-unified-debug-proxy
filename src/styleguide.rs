use anyhow::{Result, bail};
use std::fs;
use std::path::Path;

// Embedded styleguide assets per version
mod assets {
    pub mod v17 {
        pub const DTS: &str = include_str!("../styleguide/v17/ua_rt_device.d.ts");
        pub const ESLINTRC: &str = include_str!("../styleguide/v17/.eslintrc.json");
        pub const JSCONFIG: &str = include_str!("../styleguide/v17/jsconfig.json");
        pub const PACKAGE: &str = include_str!("../styleguide/v17/package.json");
        pub const DTS_FILENAME: &str = "ua_rt_device.d.ts";
    }

    pub mod v18 {
        pub const DTS: &str = include_str!("../styleguide/v18/ua_rt_device_V18.d.ts");
        pub const ESLINTRC: &str = include_str!("../styleguide/v18/.eslintrc.json");
        pub const JSCONFIG: &str = include_str!("../styleguide/v18/jsconfig.json");
        pub const PACKAGE: &str = include_str!("../styleguide/v18/package.json");
        pub const DTS_FILENAME: &str = "ua_rt_device_V18.d.ts";
    }

    pub mod v19 {
        pub const DTS: &str = include_str!("../styleguide/v19/ua_rt_device_V19.d.ts");
        pub const ESLINTRC: &str = include_str!("../styleguide/v19/.eslintrc.json");
        pub const JSCONFIG: &str = include_str!("../styleguide/v19/jsconfig.json");
        pub const PACKAGE: &str = include_str!("../styleguide/v19/package.json");
        pub const DTS_FILENAME: &str = "ua_rt_device_V19.d.ts";
    }

    pub mod v20 {
        pub const DTS: &str = include_str!("../styleguide/v20/ua_rt_device_V20.d.ts");
        pub const ESLINTRC: &str = include_str!("../styleguide/v20/.eslintrc.json");
        pub const JSCONFIG: &str = include_str!("../styleguide/v20/jsconfig.json");
        pub const PACKAGE: &str = include_str!("../styleguide/v20/package.json");
        pub const DTS_FILENAME: &str = "ua_rt_device_V20.d.ts";
    }

    pub mod v21 {
        pub const DTS: &str = include_str!("../styleguide/v21/ua_rt_device_V21.d.ts");
        pub const ESLINTRC: &str = include_str!("../styleguide/v21/.eslintrc.json");
        pub const JSCONFIG: &str = include_str!("../styleguide/v21/jsconfig.json");
        pub const PACKAGE: &str = include_str!("../styleguide/v21/package.json");
        pub const DTS_FILENAME: &str = "ua_rt_device_V21.d.ts";
    }
}

struct VersionAssets {
    dts: &'static str,
    dts_filename: &'static str,
    eslintrc: &'static str,
    jsconfig: &'static str,
    package: &'static str,
}

fn get_version_assets(version: &str) -> Result<VersionAssets> {
    match version {
        "v17" => Ok(VersionAssets {
            dts: assets::v17::DTS,
            dts_filename: assets::v17::DTS_FILENAME,
            eslintrc: assets::v17::ESLINTRC,
            jsconfig: assets::v17::JSCONFIG,
            package: assets::v17::PACKAGE,
        }),
        "v18" => Ok(VersionAssets {
            dts: assets::v18::DTS,
            dts_filename: assets::v18::DTS_FILENAME,
            eslintrc: assets::v18::ESLINTRC,
            jsconfig: assets::v18::JSCONFIG,
            package: assets::v18::PACKAGE,
        }),
        "v19" => Ok(VersionAssets {
            dts: assets::v19::DTS,
            dts_filename: assets::v19::DTS_FILENAME,
            eslintrc: assets::v19::ESLINTRC,
            jsconfig: assets::v19::JSCONFIG,
            package: assets::v19::PACKAGE,
        }),
        "v20" => Ok(VersionAssets {
            dts: assets::v20::DTS,
            dts_filename: assets::v20::DTS_FILENAME,
            eslintrc: assets::v20::ESLINTRC,
            jsconfig: assets::v20::JSCONFIG,
            package: assets::v20::PACKAGE,
        }),
        "v21" => Ok(VersionAssets {
            dts: assets::v21::DTS,
            dts_filename: assets::v21::DTS_FILENAME,
            eslintrc: assets::v21::ESLINTRC,
            jsconfig: assets::v21::JSCONFIG,
            package: assets::v21::PACKAGE,
        }),
        _ => bail!(
            "Unknown version '{}'. Supported versions: v17, v18, v19, v20, v21",
            version
        ),
    }
}

pub fn write_styleguide(version: &str, output_dir: &str) -> Result<()> {
    let assets = get_version_assets(version)?;
    let base_path = Path::new(output_dir);

    if !base_path.exists() {
        fs::create_dir_all(base_path)?;
    }

    let abs_base_path = fs::canonicalize(base_path)?;

    let files: Vec<(&str, &str)> = vec![
        (assets.dts_filename, assets.dts),
        (".eslintrc.json", assets.eslintrc),
        ("jsconfig.json", assets.jsconfig),
        ("package.json", assets.package),
    ];

    println!(
        "Writing WinCC Unified {} styleguide to {}",
        version,
        abs_base_path.display()
    );

    for (filename, content) in &files {
        let path = abs_base_path.join(filename);
        fs::write(&path, content)?;
        println!("  Created: {}", path.display());
    }

    println!();
    println!("Next steps:");
    println!("  1. npm install        (install ESLint)");
    println!("  2. npx eslint .       (lint your scripts)");
    println!("  3. Open in VS Code    (IntelliSense via jsconfig.json)");

    Ok(())
}
