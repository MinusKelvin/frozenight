use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

const NUM_FEATURES: usize = 768;
const L1_SIZE: usize = 32;
const BUCKETS: usize = 16;

#[derive(Serialize, Deserialize)]
struct A<T: Serialize + for<'d> Deserialize<'d>, const N: usize>(
    #[serde(with = "BigArray")] [T; N],
);

#[derive(Deserialize)]
pub struct Nnue {
    #[serde(rename = "ft.weight")]
    input_layer: A<A<i16, L1_SIZE>, NUM_FEATURES>,
    #[serde(rename = "ft.bias")]
    input_layer_bias: A<i16, L1_SIZE>,
    #[serde(rename = "out.weight")]
    hidden_layer: A<A<i8, { L1_SIZE * 2 }>, BUCKETS>,
    #[serde(rename = "out.bias")]
    hidden_layer_bias: A<i32, BUCKETS>,
}

fn main() {
    println!("cargo:rerun-if-changed=model.json");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=EVALFILE");

    let eval_file = std::env::var_os("EVALFILE");
    let eval_file: &Path = eval_file
        .as_ref()
        .map_or("model.json".as_ref(), |s| s.as_ref());

    let model: Nnue =
        serde_json::from_reader(BufReader::new(File::open(eval_file).unwrap())).unwrap();

    let out_dir: PathBuf = std::env::var_os("OUT_DIR").unwrap().into();
    let mut output = BufWriter::new(File::create(out_dir.join("model.rs")).unwrap());

    writeln!(output, "{}", model).unwrap();
}

impl<T, const N: usize> std::fmt::Display for A<T, N>
where
    T: std::fmt::Display + Serialize + for<'d> Deserialize<'d>,
{
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
