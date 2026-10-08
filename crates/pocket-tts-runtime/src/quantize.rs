use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};
use xn::{
    TypedTensor,
    quantized::{GgmlDType, QStorage, QTensor, gguf_file},
};
fn hash(path: &Path) -> Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut sha = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
    }
    Ok(format!("{:x}", sha.finalize()))
}
pub fn convert(input: &Path, output: &Path) -> Result<()> {
    convert_cancellable(input, output, &|| false)
}
pub fn convert_cancellable(
    input: &Path,
    output: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    ensure!(
        input != output && output.extension().is_some_and(|e| e == "gguf"),
        "derived Q8 must be a separate GGUF"
    );
    ensure!(
        !output.exists(),
        "refusing to overwrite existing derived model"
    );
    let sidecar = output.with_extension("gguf.json");
    ensure!(
        !sidecar.exists(),
        "orphan Q8 manifest exists; refusing overwrite"
    );
    ensure!(!cancelled(), "Q8 conversion cancelled");
    let input_hash = hash(input)?;
    let tensors = xn::safetensors::load_from_file(input, &xn::CPU)?;
    let mut names: Vec<_> = tensors.keys().collect();
    names.sort();
    let mut converted = Vec::with_capacity(names.len());
    let mut quantized = 0usize;
    for name in names {
        ensure!(!cancelled(), "Q8 conversion cancelled");
        let t = &tensors[name];
        let is_linear = name.starts_with("flow_lm.transformer.layers.")
            && [
                "linear1.weight",
                "linear2.weight",
                "self_attn.in_proj.weight",
                "self_attn.out_proj.weight",
            ]
            .iter()
            .any(|s| name.ends_with(s));
        let q = if is_linear {
            let f = t.to::<f32>()?;
            quantized += 1;
            QTensor::quantize_f32(&f.to_vec()?, f.shape(), GgmlDType::Q8_0)?
        } else {
            match t {
                TypedTensor::BF16(t) => {
                    QTensor::new(QStorage::Cpu(Box::new(t.to_vec()?)), t.shape().clone())?
                }
                TypedTensor::F16(t) => {
                    QTensor::new(QStorage::Cpu(Box::new(t.to_vec()?)), t.shape().clone())?
                }
                TypedTensor::F32(t) => {
                    QTensor::new(QStorage::Cpu(Box::new(t.to_vec()?)), t.shape().clone())?
                }
                _ => anyhow::bail!("unsupported tensor type at {name}"),
            }
        };
        converted.push((name.clone(), q));
    }
    ensure!(
        quantized == 24,
        "expected exactly four Q8 linears in each of six german layers"
    );
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = crate::atomic_file::Temporary::new(output.parent().unwrap())?;
    let mut writer = BufWriter::new(temporary.file.try_clone()?);
    let refs: Vec<_> = converted.iter().map(|(n, q)| (n.as_str(), q)).collect();
    gguf_file::write(&mut writer, &[], &refs)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);
    let metadata = serde_json::json!({
        "source_sha256":input_hash,"derived_sha256":hash(&temporary.path)?,"source_bytes":std::fs::metadata(input)?.len(),
        "derived_bytes":std::fs::metadata(&temporary.path)?.len(),"quantized_tensors":quantized,
        "format":"Q8_0 FlowLM transformer linears, all other source tensors preserved",
        "xn_version":"0.2.10","xn_ptts_revision":"83dfe119e4e1f7a13277aa4186882c758664ebf9",
    });
    ensure!(
        hash(input)? == input_hash,
        "original model changed during conversion"
    );
    ensure!(!cancelled(), "Q8 conversion cancelled");
    temporary.publish(output)?;
    crate::atomic_file::write_new(&sidecar, &serde_json::to_vec_pretty(&metadata)?)?;
    println!("{metadata}");
    Ok(())
}
