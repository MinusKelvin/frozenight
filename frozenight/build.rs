use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct A<T>(Vec<T>);

#[derive(Deserialize)]
pub struct Nnue {
    #[serde(rename = "ft.weight")]
    input_layer: A<A<i16>>,
    #[serde(rename = "ft.bias")]
    input_layer_bias: A<i16>,
    #[serde(rename = "out.weight")]
    hidden_layer: A<A<i8>>,
    #[serde(rename = "out.bias")]
    hidden_layer_bias: A<i32>,
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=EVALFILE");

    let eval_file = std::env::var_os("EVALFILE");
    let eval_file: &Path = eval_file
        .as_ref()
        .map_or("frozenight/model.json.zst".as_ref(), |s| s.as_ref());
    let eval_file = match eval_file.is_relative() {
        true => Path::new("..").join(eval_file),
        false => eval_file.into(),
    };
    println!("cargo:rerun-if-changed={}", eval_file.display());

    let model: Nnue = serde_json::from_reader(
        ruzstd::StreamingDecoder::new(BufReader::new(File::open(eval_file).unwrap())).unwrap(),
    )
    .unwrap();

    let out_dir: PathBuf = std::env::var_os("OUT_DIR").unwrap().into();
    let mut output = BufWriter::new(File::create(out_dir.join("model.rs")).unwrap());

    writeln!(output, "{}", model).unwrap();
}

impl<T: std::fmt::Display> std::fmt::Display for A<T> {
    fn fmt(&self, to: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(to, "[")?;
        for v in &self.0 {
            write!(to, "{},", v)?;
        }
        write!(to, "]")
    }
}

impl std::fmt::Display for Nnue {
    fn fmt(&self, to: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            to,
            "Nnue {{input_layer:{},input_layer_bias:{},hidden_layer:{},hidden_layer_bias:{}}}",
            self.input_layer, self.input_layer_bias, self.hidden_layer, self.hidden_layer_bias
        )
    }
}
