use anyhow::Result;

pub fn init_vscode(output_dir: &str, dynamics_port: u16, events_port: u16) -> Result<()> {
    use std::fs;
    use std::io::{self, Write};
    use std::path::Path;

    let base_path = Path::new(output_dir);
    let abs_base_path = fs::canonicalize(base_path).unwrap_or_else(|_| base_path.to_path_buf());
    let vscode_dir = abs_base_path.join(".vscode");
    let launch_json_path = vscode_dir.join("launch.json");

    let launch_json = format!(
        r#"{{
    "version": "0.2.0",
    "configurations": [
        {{
            "type": "node",
            "request": "attach",
            "name": "WinCC:Dynamics",
            "address": "localhost",
            "port": {},
            "restart": true,
            "timeout": 30000,
            "sourceMaps": true,
            "resolveSourceMapLocations": ["**", "!**/node_modules/**"],
            "skipFiles": ["<node_internals>/**"],
            "smartStep": true,
            "pauseForSourceMap": true
        }},
        {{
            "type": "node",
            "request": "attach",
            "name": "WinCC:Events",
            "address": "localhost",
            "port": {},
            "restart": true,
            "timeout": 30000,
            "sourceMaps": true,
            "resolveSourceMapLocations": ["**", "!**/node_modules/**"],
            "skipFiles": ["<node_internals>/**"],
            "smartStep": true,
            "pauseForSourceMap": true
        }}
    ],
    "compounds": [
        {{
            "name": "WinCC:All",
            "configurations": ["WinCC:Dynamics", "WinCC:Events"],
            "stopAll": true
        }}
    ]
}}"#,
        dynamics_port, events_port
    );

    // Check if launch.json already exists
    if launch_json_path.exists() {
        println!("Warning: {} already exists!", launch_json_path.display());
        println!("Please merge the following configuration manually:");
        println!();
        println!("{}", launch_json);
        return Ok(());
    }

    // Ask for confirmation
    print!("Create launch.json in {}? [Y/n] ", vscode_dir.display());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input == "n" || input == "no" {
        println!();
        println!("Copy this configuration to your launch.json:");
        println!();
        println!("{}", launch_json);
        return Ok(());
    }

    // Create .vscode directory if it doesn't exist
    if !vscode_dir.exists() {
        fs::create_dir_all(&vscode_dir)?;
    }

    fs::write(&launch_json_path, &launch_json)?;
    println!();
    println!("Created: {}", launch_json_path.display());
    println!();
    println!("VS Code debug configurations added:");
    println!("  - WinCC:Dynamics (port {})", dynamics_port);
    println!("  - WinCC:Events (port {})", events_port);
    println!("  - WinCC:All (both)");

    Ok(())
}

pub fn generate_netsh_scripts(address: &str, port: u16, output_dir: &str) -> Result<()> {
    use std::fs;
    use std::io::{self, Write};
    use std::path::Path;

    let base_path = Path::new(output_dir);
    if !base_path.exists() {
        fs::create_dir_all(base_path)?;
    }
    let abs_base_path = fs::canonicalize(base_path)?;
    let addr_slug = address.replace('.', "-");

    let files: Vec<(String, String)> = vec![
        (
            format!("wincc-debug-setup-{addr_slug}.bat"),
            format!(
                r#"@echo off
echo Setting up WinCC remote debug on {address}:{port}...

:: Remove existing rules (safe if they don't exist)
netsh interface portproxy delete v4tov4 listenaddress={address} listenport={port}
netsh advfirewall firewall delete rule name="WinCC Debug {port} IN" >nul 2>&1
netsh advfirewall firewall delete rule name="WinCC Debug {port} OUT" >nul 2>&1

:: Add port proxy and firewall rules
netsh interface portproxy add v4tov4 listenaddress={address} listenport={port} connectaddress=127.0.0.1 connectport={port}
netsh advfirewall firewall add rule name="WinCC Debug {port} IN" dir=in action=allow protocol=tcp localport={port}
netsh advfirewall firewall add rule name="WinCC Debug {port} OUT" dir=out action=allow protocol=tcp localport={port}

echo Done! Port proxy and firewall rules configured.
pause
"#,
                address = address,
                port = port
            ),
        ),
        (
            format!("wincc-debug-restart-{addr_slug}.bat"),
            format!(
                r#"@echo off
echo Fixing WinCC remote debug port proxy for {address}:{port} (post-restart fix)...

netsh interface portproxy delete v4tov4 listenaddress={address} listenport={port}
netsh interface portproxy add v4tov4 listenaddress={address} listenport={port} connectaddress=127.0.0.1 connectport={port}

echo Done! Port proxy rule re-applied.
pause
"#,
                address = address,
                port = port
            ),
        ),
        (
            format!("wincc-debug-cleanup-{addr_slug}.bat"),
            format!(
                r#"@echo off
echo Removing WinCC remote debug rules for {address}:{port}...

netsh interface portproxy delete v4tov4 listenaddress={address} listenport={port}
netsh advfirewall firewall delete rule name="WinCC Debug {port} IN"
netsh advfirewall firewall delete rule name="WinCC Debug {port} OUT"

echo Done! All rules removed.
pause
"#,
                address = address,
                port = port
            ),
        ),
    ];

    // Check for existing files
    let existing: Vec<&str> = files
        .iter()
        .filter(|(name, _)| abs_base_path.join(name).exists())
        .map(|(name, _)| name.as_str())
        .collect();

    if !existing.is_empty() {
        println!("Warning: the following files already exist in {}:", abs_base_path.display());
        for name in &existing {
            let path = abs_base_path.join(name);
            println!("\n  {}", path.display());
            println!("  Current contents:");
            if let Ok(contents) = fs::read_to_string(&path) {
                for line in contents.lines() {
                    println!("    {}", line);
                }
            }
        }
        println!();
        print!("Overwrite existing files? [Y/n] ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input == "n" || input == "no" {
            println!("Aborted.");
            return Ok(());
        }
    } else {
        print!(
            "Generate netsh .bat scripts for {}:{} in {}? [Y/n] ",
            address,
            port,
            abs_base_path.display()
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input == "n" || input == "no" {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Write all files
    println!();
    for (name, content) in &files {
        let path = abs_base_path.join(name);
        fs::write(&path, content)?;
        println!("Created: {}", path.display());
    }

    println!();
    println!("Run these .bat files as Administrator on the WinCC machine:");
    println!("  wincc-debug-setup-{}.bat   - First-time setup (port proxy + firewall rules)", addr_slug);
    println!("  wincc-debug-restart-{}.bat - After Windows restart (re-apply port proxy)", addr_slug);
    println!("  wincc-debug-cleanup-{}.bat - Remove all rules", addr_slug);

    Ok(())
}
