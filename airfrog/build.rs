// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! build.rs for default airfrog firmware

use jiff::Timestamp;
use std::path::Path;
use std::{env, fs};

use swc_common::GLOBALS;
use swc_common::{FileName, Mark, SourceMap, sync::Lrc};
use swc_ecma_ast::{EsVersion, Program};
use swc_ecma_codegen::{Config, Emitter, text_writer::JsWriter};
use swc_ecma_minifier::{
    optimize,
    option::{ExtraOptions, MinifyOptions},
};
use swc_ecma_parser::{Syntax, parse_file_as_module};

const PART_CSV: &str = "partitions.csv";
const PART_RUST: &str = "partitions.rs";

fn main() {
    println!("cargo:rerun-if-env-changed=ESP_LOG");
    println!("cargo:rerun-if-env-changed=SSID");
    println!("cargo:rerun-if-env-changed=PASSWORD");
    println!("cargo:rerun-if-env-changed=DISABLE_WIFI");
    println!("cargo:rerun-if-env-changed=MQTT_BROKER_IP");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=build.rs");

    // Get build time and date
    //
    // Use the same source and formatting as esp-bootloader-esp-idf as we pass
    // these values into the esp_app_desc! macro.
    let build_time = Timestamp::now();
    let build_time_formatted = build_time.strftime("%H:%M:%S");
    let build_date_formatted = build_time.strftime("%Y-%m-%d");
    println!("cargo::rustc-env=AIRFROG_BUILD_TIME={build_time_formatted}");
    println!("cargo::rustc-env=AIRFROG_BUILD_DATE={build_date_formatted}");

    linker_be_nice();
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");

    built::write_built_file().expect("Failed to acquire build-time information");

    // minify HTML and JS files
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);
    minify_html_js("assets", out_path).unwrap();
    minify_html_js("assets/firmware", out_path).unwrap();

    // Parse our partition table
    println!("cargo:rerun-if-changed={PART_CSV}");
    parse_partitions_csv();
}

fn parse_partitions_csv() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join(PART_RUST);

    // Read the CSV file
    let csv_content = fs::read_to_string(PART_CSV)
        .unwrap_or_else(|_| panic!("Failed to read {PART_CSV}"))
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(csv_content.as_bytes());

    // Collect consts for each partition
    let mut constants = Vec::new();
    let mut partition_count = 0;
    for (index, result) in reader.records().enumerate() {
        let record = result.expect("Failed to parse CSV record");

        // Get the partition information, removing leading whitespace
        let partition_name = &record[0].trim();
        let offset_str = &record[3].trim();
        let size_str = &record[4].trim();

        // Convert to constant name
        let const_name = partition_name.to_uppercase();

        // Parse hex values (strip 0x prefix)
        let _offset = u32::from_str_radix(&offset_str[2..], 16)
            .unwrap_or_else(|_| panic!("Failed to parse offset: {offset_str}"));
        let _size = u32::from_str_radix(&size_str[2..], 16)
            .unwrap_or_else(|_| panic!("Failed to parse size: {size_str}"));

        constants.push(format!(
            "#[allow(dead_code)]\nconst PART_INDEX_{const_name}: usize = {index};",
        ));
        constants.push(format!(
            "#[allow(dead_code)]\nconst PART_OFFSET_{const_name}: u32 = {offset_str};",
        ));
        constants.push(format!(
            "#[allow(dead_code)]\nconst PART_SIZE_{const_name}: u32 = {size_str};",
        ));
        partition_count += 1;
    }

    constants.push(format!(
        "#[allow(dead_code)]\nconst PART_COUNT: usize = {partition_count};",
    ));

    let output_content = constants.join("\n") + "\n";
    fs::write(&out_path, output_content).expect("Failed to write partition constants");
}

fn minify_html_js(dir: &str, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            minify_html_js(&path.to_string_lossy(), out_dir)?;
        } else if let Some(ext) = path.extension() {
            match ext.to_str() {
                Some("html") | Some("htm") | Some("css") => {
                    if let Err(e) = minify_html(&path, out_dir) {
                        eprintln!("HTML minify failed for {}: {}", path.display(), e);
                    }
                }
                Some("js") => {
                    if let Err(e) = minify_js(&path, out_dir) {
                        eprintln!("JS processing failed for {}: {}", path.display(), e);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn minify_html(path: &Path, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let cfg = minify_html::Cfg::default();
    let minified = minify_html::minify(content.as_bytes(), &cfg);

    let relative_path = path.strip_prefix("assets")?;
    let output_path = out_dir.join(relative_path);

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&output_path, minified)?;
    println!(
        "Minified HTML: {} -> {}",
        path.display(),
        output_path.display()
    );
    Ok(())
}

fn minify_js(path: &Path, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let relative_path = path.strip_prefix("assets")?;
    let output_path = out_dir.join(relative_path);

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    println!("Processing JS: {}", path.display());

    // Try SWC minification first
    match minify_with_swc(&content) {
        Ok(minified) => {
            fs::write(&output_path, minified)?;
            println!(
                "Minified JS (SWC): {} -> {}",
                path.display(),
                output_path.display()
            );
        }
        Err(e) => {
            eprintln!("SWC minify failed for {}: {:?}", path.display(), e);
            fs::write(&output_path, content)?;
            println!(
                "Copied JS (fallback): {} -> {}",
                path.display(),
                output_path.display()
            );
        }
    }

    Ok(())
}

fn minify_with_swc(content: &str) -> Result<String, Box<dyn std::error::Error>> {
    GLOBALS.set(&Default::default(), || {
        let cm = Lrc::new(SourceMap::default());

        let source_file = cm.new_source_file(FileName::Anon.into(), content.to_string());

        let module = parse_file_as_module(
            &source_file,
            Syntax::Es(Default::default()),
            EsVersion::Es2020,
            None,
            &mut vec![],
        )
        .map_err(|e| format!("Parse error: {e:?}"))?;

        let program = Program::Module(module);
        let top_level_mark = Mark::fresh(Mark::root());

        let minified = optimize(
            program,
            cm.clone(),
            None,
            None,
            &MinifyOptions {
                compress: None,
                mangle: None,
                ..Default::default()
            },
            &ExtraOptions {
                unresolved_mark: Mark::fresh(Mark::root()),
                top_level_mark,
                mangle_name_cache: None,
            },
        );

        let mut buf = vec![];
        let writer = JsWriter::new(cm.clone(), "", &mut buf, None);
        let mut cfg = Config::default();
        cfg.minify = true;
        let mut emitter = Emitter {
            cfg,
            cm,
            comments: None,
            wr: writer,
        };

        match &minified {
            Program::Module(m) => emitter.emit_module(m),
            Program::Script(s) => emitter.emit_script(s),
        }
        .map_err(|e| format!("Codegen error: {e:?}"))?;

        Ok(String::from_utf8(buf)?)
    })
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                "_defmt_timestamp" => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                "esp_wifi_preempt_enable"
                | "esp_wifi_preempt_yield_task"
                | "esp_wifi_preempt_task_create" => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-wifi` has no scheduler enabled. Make sure you have the `builtin-scheduler` feature enabled, or that you provide an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
