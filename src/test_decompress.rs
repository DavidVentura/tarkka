use std::io::Cursor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let compressed_group = std::fs::read("out2")?;
    let mut decompressed = Vec::new();
    {
        let cursor = Cursor::new(compressed_group);
        let mut decoder = zeekstd::Decoder::new(cursor)?;
        println!("deco");
        std::io::copy(&mut decoder, &mut decompressed)?;
    }
    Ok(())
}
