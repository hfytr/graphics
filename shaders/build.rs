use spirv_builder::{MetadataPrintout, SpirvBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    SpirvBuilder::new("shader-crate", "spirv-unknown-spv1.0")
        .print_metadata(MetadataPrintout::Full)
        .build()?;
    Ok(())
}
