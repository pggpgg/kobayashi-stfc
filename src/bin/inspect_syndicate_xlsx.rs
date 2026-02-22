//! Inspect a Syndicate Progression .xlsx: print sheet names and first rows of the data sheet.
//! Usage: cargo run --bin inspect_syndicate_xlsx -- path/to/Syndicate Progression.xlsx

use std::path::Path;

use calamine::Reader;

fn cell_str(d: &calamine::Data) -> String {
    match d {
        calamine::Data::Empty => String::new(),
        calamine::Data::String(s) => s.clone(),
        calamine::Data::Float(f) => format!("{}", f),
        calamine::Data::Int(i) => format!("{}", i),
        calamine::Data::Bool(b) => format!("{}", b),
        _ => format!("{:?}", d),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: inspect_syndicate_xlsx <path-to.xlsx>")?;
    let path = Path::new(&path);
    if !path.exists() {
        return Err(format!("File not found: {}", path.display()).into());
    }

    let mut wb = calamine::open_workbook_auto(path)?;
    let names = wb.sheet_names();
    println!("Sheets ({}): {}", names.len(), names.join(", "));

    let sheet_name = names
        .iter()
        .find(|s: &&String| s.contains("Level By Level") || s.contains("Comparison"))
        .or(names.first())
        .ok_or("No sheets")?;
    println!("\nUsing sheet: {}", sheet_name);

    let range = wb.worksheet_range(sheet_name)?;
    let (height, width) = range.get_size();
    println!("Size: {} rows x {} cols\nFirst 25 rows:", height, width);

    for (i, row) in range.rows().take(25).enumerate() {
        let cells: Vec<String> = row.iter().map(|c| cell_str(c)).collect();
        println!("  {}: {}", i, cells.join(" | "));
    }
    Ok(())
}
