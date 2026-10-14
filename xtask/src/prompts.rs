//! `cargo xtask prompts <bpe_simple_vocab_16e6.txt.gz> <textual.onnx>` :
//! calcule avec l'encodeur de texte de CLIP RN50 l'empreinte de chaque
//! description de `src/moderation/prompts.rs`, et l'écrit dans
//! `src/moderation/prompts.bin`. Le serveur n'a ensuite besoin que de
//! l'encodeur d'images.
//!
//! Le découpage en jetons est celui de CLIP (BPE sur octets, 49 408 jetons),
//! réécrit ici en Rust.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use tract_onnx::prelude::*;

#[path = "../../src/moderation/prompts.rs"]
mod list;

const CONTEXT: usize = 77;
const START: i32 = 49406;
const END: i32 = 49407;

/// Le découpage de CLIP.
pub struct Tokenizer {
    encoder: HashMap<String, i32>,
    ranks: HashMap<(String, String), usize>,
    byte_encoder: [char; 256],
    pattern: regex::Regex,
}

/// Les octets vers des caractères imprimables, comme GPT-2 et CLIP, dans
/// l'ordre du vocabulaire (les imprimables d'abord).
fn bytes_to_unicode() -> Vec<(u8, char)> {
    let mut bs: Vec<u32> = (b'!' as u32..=b'~' as u32)
        .chain(0xA1..=0xAC)
        .chain(0xAE..=0xFF)
        .collect();
    let mut cs = bs.clone();
    let mut n = 0;
    for b in 0..256u32 {
        if !bs.contains(&b) {
            bs.push(b);
            cs.push(256 + n);
            n += 1;
        }
    }
    bs.iter()
        .zip(cs)
        .map(|(b, c)| (*b as u8, char::from_u32(c).unwrap()))
        .collect()
}

impl Tokenizer {
    pub fn new(vocab_gz: &[u8]) -> Result<Tokenizer, String> {
        let mut text = String::new();
        flate2::read::GzDecoder::new(vocab_gz)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        let merges: Vec<(String, String)> = text
            .lines()
            .skip(1)
            .take(49152 - 256 - 2)
            .map(|l| {
                let (a, b) = l.split_once(' ').expect("fusion");
                (a.to_owned(), b.to_owned())
            })
            .collect();
        let order = bytes_to_unicode();
        let mut byte_encoder = ['\0'; 256];
        for (b, c) in &order {
            byte_encoder[*b as usize] = *c;
        }
        let mut vocab: Vec<String> = order.iter().map(|(_, c)| c.to_string()).collect();
        vocab.extend(order.iter().map(|(_, c)| format!("{c}</w>")));
        vocab.extend(merges.iter().map(|(a, b)| format!("{a}{b}")));
        vocab.push("<|startoftext|>".into());
        vocab.push("<|endoftext|>".into());
        let encoder = vocab
            .into_iter()
            .enumerate()
            .map(|(i, t)| (t, i as i32))
            .collect();
        let ranks = merges
            .into_iter()
            .enumerate()
            .map(|(i, m)| (m, i))
            .collect();
        let pattern = regex::Regex::new(
            r"<\|startoftext\|>|<\|endoftext\|>|'s|'t|'re|'ve|'m|'ll|'d|[\p{L}]+|[\p{N}]|[^\s\p{L}\p{N}]+",
        )
        .map_err(|e| e.to_string())?;
        Ok(Tokenizer {
            encoder,
            ranks,
            byte_encoder,
            pattern,
        })
    }

    fn bpe(&self, token: &str) -> Vec<String> {
        let chars: Vec<char> = token.chars().collect();
        let mut word: Vec<String> = chars.iter().map(|c| c.to_string()).collect();
        if let Some(last) = word.last_mut() {
            last.push_str("</w>");
        }
        loop {
            let best = word
                .windows(2)
                .filter_map(|p| {
                    self.ranks
                        .get(&(p[0].clone(), p[1].clone()))
                        .map(|r| (*r, p))
                })
                .min_by_key(|(r, _)| *r)
                .map(|(_, p)| (p[0].clone(), p[1].clone()));
            let Some((a, b)) = best else { break };
            let mut merged = Vec::with_capacity(word.len());
            let mut i = 0;
            while i < word.len() {
                if i + 1 < word.len() && word[i] == a && word[i + 1] == b {
                    merged.push(format!("{a}{b}"));
                    i += 2;
                } else {
                    merged.push(word[i].clone());
                    i += 1;
                }
            }
            word = merged;
            if word.len() == 1 {
                break;
            }
        }
        word
    }

    /// Les jetons d'une phrase, encadrés et complétés à 77.
    pub fn encode(&self, text: &str) -> Vec<i32> {
        let clean = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let mut ids = vec![START];
        for m in self.pattern.find_iter(&clean) {
            let token: String = m
                .as_str()
                .bytes()
                .map(|b| self.byte_encoder[b as usize])
                .collect();
            for piece in self.bpe(&token) {
                ids.push(self.encoder[&piece]);
            }
        }
        ids.truncate(CONTEXT - 1);
        ids.push(END);
        ids
    }
}

pub fn compute(vocab: &Path, textual: &Path, out: &Path) -> Result<(), String> {
    let err = |e: &dyn std::fmt::Display| e.to_string();
    let tokenizer = Tokenizer::new(&std::fs::read(vocab).map_err(|e| err(&e))?)?;
    let model = tract_onnx::onnx()
        .model_for_path(textual)
        .and_then(|m| m.with_input_fact(0, i32::fact([1, CONTEXT]).into()))
        .and_then(|m| m.with_input_fact(1, i32::fact([1, CONTEXT]).into()))
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(|e| err(&e))?;
    let mut bin = Vec::new();
    bin.extend_from_slice(list::MAGIC);
    bin.extend_from_slice(&list::digest());
    for (group, text) in list::PROMPTS {
        let ids = tokenizer.encode(text);
        let mut input = vec![0i32; CONTEXT];
        let mut mask = vec![0i32; CONTEXT];
        input[..ids.len()].copy_from_slice(&ids);
        mask[..ids.len()].fill(1);
        let ids_t: Tensor = tract_ndarray::Array2::from_shape_vec((1, CONTEXT), input)
            .map_err(|e| err(&e))?
            .into();
        let mask_t: Tensor = tract_ndarray::Array2::from_shape_vec((1, CONTEXT), mask)
            .map_err(|e| err(&e))?
            .into();
        let result = model
            .run(tvec!(ids_t.into(), mask_t.into()))
            .map_err(|e| err(&e))?;
        let view = result[0]
            .to_plain_array_view::<f32>()
            .map_err(|e| err(&e))?;
        let v: Vec<f32> = view.iter().copied().collect();
        if v.len() != list::DIM {
            return Err(format!("dimension inattendue : {}", v.len()));
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in v {
            bin.extend_from_slice(&(x / norm).to_le_bytes());
        }
        println!("{group:?} · {text}");
    }
    std::fs::write(out, &bin).map_err(|e| err(&e))?;
    println!("{} : {} descriptions", out.display(), list::PROMPTS.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn bytes_table_is_a_bijection() {
        let table = super::bytes_to_unicode();
        let mut seen: Vec<char> = table.iter().map(|(_, c)| *c).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 256);
    }
}
