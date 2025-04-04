use anyhow::{Context, Result};
use clap::Parser;
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use std::{fs, path::Path};
use tokio::fs as tokio_fs;

#[derive(Parser, Debug)]
#[command(
    name = "assemble-malie-dzi",
    version,
    about = "A tool to assemble malie engine layers."
)]
struct Cli {
    #[arg(short, long, default_value = "event")]
    event_dir: String,

    #[arg(short, long, default_value = "tex")]
    tex_dir: String,

    #[arg(short, long, default_value = "dist")]
    output_dir: String,

    #[arg(long, default_value_t = true)]
    enable_lower_layers: bool,
}

#[allow(dead_code)]
#[derive(Debug)]
struct DziLayer {
    tiles: Vec<Vec<String>>,
    rows: usize,
    cols: usize,
}

#[derive(Debug)]
struct DziFile {
    width: u32,
    height: u32,
    layers: Vec<DziLayer>,
}

async fn parse_dzi(file_path: &Path) -> Result<DziFile> {
    let content = tokio_fs::read_to_string(file_path).await?;
    let mut lines = content.lines();

    let _format_line = lines.next();
    let size_line = lines.next().context("No size line in DZI file")?;
    let (img_width, img_height) = {
        let parts: Vec<u32> = size_line
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();
        (parts[0], parts[1])
    };

    let mut layers = Vec::new();

    let mut iter = lines.peekable();
    while iter.peek().is_some() {
        let size_line = iter.next().unwrap();
        let parts: Vec<usize> = size_line
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();
        let (cols, rows) = (parts[0], parts[1]);

        let mut tiles = Vec::new();
        for _ in 0..rows {
            let row_line = iter.next().unwrap();
            let row: Vec<String> = row_line.split(',').map(|s| s.trim().to_string()).collect();
            tiles.push(row);
        }

        layers.push(DziLayer { tiles, rows, cols });
    }

    Ok(DziFile {
        width: img_width,
        height: img_height,
        layers,
    })
}
fn load_tile(path: &Path) -> Result<DynamicImage> {
    image::open(path).with_context(|| format!("Failed to open tile: {:?}", path))
}

fn compose_layer(
    tiles: &[Vec<String>],
    layer_index: usize,
    group: &str,
    tex_dir: &Path,
    output_path: &Path,
    final_width: u32,
    final_height: u32,
) -> Result<()> {
    if tiles.is_empty() || tiles[0].is_empty() {
        return Ok(());
    }

    let first_tile_path = tex_dir.join(format!("{}.png", tiles[0][0]));
    let first_tile = load_tile(&first_tile_path)?;
    let (tile_w, tile_h) = first_tile.dimensions();

    let rows = tiles.len();
    let cols = tiles[0].len();
    let composed_w = cols as u32 * tile_w;
    let composed_h = rows as u32 * tile_h;

    let mut canvas = RgbaImage::from_pixel(composed_w, composed_h, Rgba([0, 0, 0, 0]));

    for (y, row) in tiles.iter().enumerate() {
        for (x, tile_name) in row.iter().enumerate() {
            if tile_name.is_empty() {
                continue;
            }
            let tile_path = tex_dir.join(format!("{tile_name}.png"));
            let tile_img = load_tile(&tile_path)?.to_rgba8();

            image::imageops::overlay(
                &mut canvas,
                &tile_img,
                (x as u32 * tile_w) as i64,
                (y as u32 * tile_h) as i64,
            );
        }
    }

    let cropped = image::imageops::crop_imm(
        &canvas,
        0,
        0,
        final_width.min(composed_w),
        final_height.min(composed_h),
    )
    .to_image();

    let out_dir = output_path.join(group);
    fs::create_dir_all(&out_dir)?;
    let out_file = out_dir.join(format!("layer_{layer_index}.png"));
    cropped.save(out_file)?;

    println!("Composed layer_{layer_index} for group {group}");
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("\nERROR: {err}");
        eprintln!("Ensure the `--event-dir`、`--tex-dir` path exist!");
        eprintln!("e.g.: rust_impl.exe --event-dir <event> --tex-dir <tex> --output-dir <dist> --enable-lower-layers <true|false>");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let event_dir = Path::new(&cli.event_dir);
    let tex_dir = event_dir.join(&cli.tex_dir);
    let output_path = event_dir.join(&cli.output_dir);

    if output_path.exists() {
        fs::remove_dir_all(&output_path)?;
    }

    for entry in fs::read_dir(event_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("dzi") {
            continue;
        }

        let group = path.file_stem().unwrap().to_string_lossy().to_string();
        println!("Handling {group} ...");

        let dzi = parse_dzi(&path).await?;

        for (i, layer) in dzi.layers.iter().enumerate() {
            if i > 1 && !cli.enable_lower_layers {
                println!("Skipping layer_{i} due to config");
                continue;
            }

            let scale = 0.5_f64.powi((i as i32) - 1);
            let target_w = (dzi.width as f64 * scale).round() as u32;
            let target_h = (dzi.height as f64 * scale).round() as u32;

            compose_layer(
                &layer.tiles,
                i,
                &group,
                &tex_dir,
                &output_path,
                target_w,
                target_h,
            )?;
        }
    }

    println!("Assemble all cgs successfully!");
    Ok(())
}
