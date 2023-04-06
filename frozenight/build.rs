use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct A<T>(Vec<T>);

#[derive(Deserialize)]
pub struct Nnue {
    #[serde(rename = "ft.weight")]
    input_layer: A<A<i16>>,
    #[serde(rename = "ft.bias")]
    input_layer_bias: A<i16>,
    #[serde(rename = "hidden.weight")]
    hidden_layer: A<A<i8>>,
    #[serde(rename = "hidden.bias")]
    hidden_layer_bias: A<i32>,
    #[serde(rename = "out.weight")]
    output_layer: A<A<i8>>,
    #[serde(rename = "out.bias")]
    output_layer_bias: A<i32>,
}

#[derive(Deserialize)]
pub struct LayerStack {
    hidden_layer: A<A<i8>>,
    hidden_layer_bias: A<i32>,
    output_layer: A<i8>,
    output_layer_bias: i32,
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
    let eval_file = eval_file.canonicalize().unwrap();
    println!("cargo:rerun-if-changed={}", eval_file.display());

    let model: Nnue = serde_json::from_reader(
        ruzstd::StreamingDecoder::new(BufReader::new(File::open(eval_file).unwrap())).unwrap(),
    )
    .unwrap();

    let mut backends = vec![];
    let hidden = model.hidden_layer.0.chunks(8);
    let hidden_bias = model.hidden_layer_bias.0.chunks(8);
    let output = model.output_layer.0.iter();
    let output_bias = model.output_layer_bias.0.iter();

    for ((h, hb), (o, ob)) in hidden.zip(hidden_bias).zip(output.zip(output_bias)) {
        // let mut inner = vec![A(vec![0; h.len()]); h[0].0.len()];
        // for i in 0..h.len() {
        //     for j in 0..h[i].0.len() {
        //         inner[j].0[i] = h[i].0[j];
        //     }
        // }
        let mut hb = hb.to_vec();
        hb.iter_mut().for_each(|v| *v *= 127);
        backends.push(LayerStack {
            hidden_layer: A(h.to_vec()),
            hidden_layer_bias: A(hb),
            output_layer: o.clone(),
            output_layer_bias: *ob * 127,
        });
    }

    let out_dir: PathBuf = std::env::var_os("OUT_DIR").unwrap().into();
    let mut output = BufWriter::new(File::create(out_dir.join("model.rs")).unwrap());

    writeln!(
        output,
        "Nnue {{input_layer:{},input_layer_bias:{},backend:{}}}",
        model.input_layer,
        model.input_layer_bias,
        A(backends)
    )
    .unwrap();
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

impl std::fmt::Display for LayerStack {
    fn fmt(&self, to: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            to,
            "LayerStack {{hidden_layer:{},hidden_layer_bias:{},output_layer:{},output_layer_bias:{}}}",
            self.hidden_layer,
            self.hidden_layer_bias,
            self.output_layer,
            self.output_layer_bias
        )
    }
}
