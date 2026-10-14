//! La modération : le contenu sexuel et la drogue sont interdits, et une image
//! à caractère sexuel ou montrant de la drogue **bloque l'adresse IP** de qui
//! l'envoie.
//!
//! - Le **texte** reste chiffré de bout en bout : c'est le navigateur qui le
//!   filtre ([`alley_moderation`]), à l'envoi comme à la réception. Le serveur
//!   filtre les noms de salon, qu'il voit.
//! - Une **image** est vérifiée par le serveur au passage : l'expéditeur lui
//!   confie la clé de cette seule image, il la déchiffre en mémoire, la juge
//!   avec deux modèles, puis l'oublie. Rien n'est enregistré.
//!   - un classifieur d'images sexuelles (MobileNet v2 de GantMan) ;
//!   - CLIP (RN50), qui compare l'image aux descriptions de [`prompts`] :
//!     drogues, nudité, et beaucoup d'images ordinaires.
//! - Un **blocage** dure 30 jours (`ALLEY_BAN_DAYS`), pour l'adresse (ou son
//!   /64 en IPv6) : toutes ses connexions sont fermées, toutes ses requêtes
//!   refusées (le registre `AutoBan` de Securite).
//!
//! Les modèles sont dans `models/` (`ALLEY_MODELS`), téléchargés par
//! `cargo xtask models`. Sans eux, les images sont refusées : aucune image
//! n'est relayée sans avoir été vérifiée.

pub mod prompts;

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use image::imageops::FilterType;
use paquets::securite::prelude::AutoBan;
use tract_onnx::prelude::*;

pub use alley_moderation::{Category, check_text};
use prompts::{DIM, Group, MAGIC, PROMPTS};

type Plan = Arc<TypedRunnableModel>;

/// Taille maximale d'une image (côté le plus long, en pixels).
const MAX_SIDE: u32 = 8192;

/// Le verdict sur une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Relayée.
    Clean,
    /// Refusée (douteuse), sans blocage.
    Refused(Category),
    /// Refusée, et l'expéditeur est bloqué.
    Banned(Category),
    /// Ce n'est pas une image JPEG lisible.
    Invalid,
    /// Les modèles ne sont pas chargés : refusée par prudence.
    Unavailable,
}

/// Ce que les modèles voient dans une image (probabilités entre 0 et 1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Scores {
    /// Pornographie et hentai (classifieur d'images sexuelles).
    pub explicit: f32,
    /// Suggestif (sous-vêtements, poses).
    pub suggestive: f32,
    /// Nudité selon CLIP.
    pub nudity: f32,
    /// Drogue selon CLIP.
    pub drugs: f32,
}

impl Scores {
    /// Les seuils : bloquer seulement quand c'est net, refuser dès le doute.
    pub fn verdict(&self) -> Verdict {
        if self.explicit >= 0.80 || (self.explicit >= 0.50 && self.nudity >= 0.60) {
            return Verdict::Banned(Category::Sexual);
        }
        if self.drugs >= 0.75 {
            return Verdict::Banned(Category::Drugs);
        }
        if self.explicit + self.suggestive >= 0.60 || self.nudity >= 0.60 {
            return Verdict::Refused(Category::Sexual);
        }
        if self.drugs >= 0.45 {
            return Verdict::Refused(Category::Drugs);
        }
        Verdict::Clean
    }
}

/// Les deux modèles d'images.
pub struct Images {
    nsfw: Plan,
    clip: Plan,
    prompts: Vec<(Group, Vec<f32>)>,
}

/// Les empreintes des descriptions, calculées par `cargo xtask prompts`.
const PROMPTS_BIN: &[u8] = include_bytes!("prompts.bin");

fn load_prompts() -> Result<Vec<(Group, Vec<f32>)>, String> {
    let (head, rest) = PROMPTS_BIN
        .split_at_checked(MAGIC.len() + 32)
        .ok_or("prompts.bin trop court")?;
    if &head[..MAGIC.len()] != MAGIC || head[MAGIC.len()..] != prompts::digest() {
        return Err(
            "prompts.bin ne correspond pas à prompts.rs : relancez `cargo xtask prompts`".into(),
        );
    }
    if rest.len() != PROMPTS.len() * DIM * 4 {
        return Err("prompts.bin : taille inattendue".into());
    }
    let floats: Vec<f32> = rest
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    Ok(PROMPTS
        .iter()
        .zip(floats.chunks(DIM))
        .map(|((group, _), v)| (*group, v.to_vec()))
        .collect())
}

fn plan(path: &Path, shape: [usize; 4]) -> Result<Plan, String> {
    tract_onnx::onnx()
        .model_for_path(path)
        .and_then(|m| m.with_input_fact(0, f32::fact(shape).into()))
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(|e| format!("{} : {e}", path.display()))
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().copied().fold(f32::MIN, f32::max);
    let exp: Vec<f32> = logits.iter().map(|x| (x - max).exp()).collect();
    let sum: f32 = exp.iter().sum();
    exp.into_iter().map(|x| x / sum).collect()
}

impl Images {
    pub fn load(dir: &Path) -> Result<Images, String> {
        Ok(Images {
            nsfw: plan(&dir.join("nsfw.onnx"), [1, 224, 224, 3])?,
            clip: plan(&dir.join("clip-visual.onnx"), [1, 3, 224, 224])?,
            prompts: load_prompts()?,
        })
    }

    /// Ce que les modèles voient dans `image`.
    pub fn scores(&self, image: &image::RgbImage) -> Result<Scores, String> {
        let err = |e: TractError| e.to_string();
        // Le classifieur d'images sexuelles : 224 × 224, NHWC, entre 0 et 1.
        let small = image::imageops::resize(image, 224, 224, FilterType::Triangle);
        let input: Tensor =
            tract_ndarray::Array4::from_shape_fn((1, 224, 224, 3), |(_, y, x, c)| {
                small[(x as u32, y as u32)][c] as f32 / 255.0
            })
            .into();
        let out = self.nsfw.run(tvec!(input.into())).map_err(err)?;
        let p: Vec<f32> = out[0]
            .to_plain_array_view::<f32>()
            .map_err(err)?
            .iter()
            .copied()
            .collect();
        // drawings, hentai, neutral, porn, sexy
        let (hentai, porn, sexy) = (p[1], p[3], p[4]);

        // CLIP : le petit côté à 224, recadré au centre, normalisé.
        let (w, h) = image.dimensions();
        let scale = 224.0 / w.min(h) as f32;
        let (rw, rh) = (
            ((w as f32 * scale).round() as u32).max(224),
            ((h as f32 * scale).round() as u32).max(224),
        );
        let resized = image::imageops::resize(image, rw, rh, FilterType::CatmullRom);
        let (ox, oy) = ((rw - 224) / 2, (rh - 224) / 2);
        const MEAN: [f32; 3] = [0.481_454_66, 0.457_827_5, 0.408_210_73];
        const STD: [f32; 3] = [0.268_629_54, 0.261_302_6, 0.275_777_1];
        let input: Tensor =
            tract_ndarray::Array4::from_shape_fn((1, 3, 224, 224), |(_, c, y, x)| {
                let v = resized[(ox + x as u32, oy + y as u32)][c] as f32 / 255.0;
                (v - MEAN[c]) / STD[c]
            })
            .into();
        let out = self.clip.run(tvec!(input.into())).map_err(err)?;
        let v: Vec<f32> = out[0]
            .to_plain_array_view::<f32>()
            .map_err(err)?
            .iter()
            .copied()
            .collect();
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
        let logits: Vec<f32> = self
            .prompts
            .iter()
            .map(|(_, p)| 100.0 * p.iter().zip(&v).map(|(a, b)| a * b).sum::<f32>() / norm)
            .collect();
        let probs = softmax(&logits);
        let sum = |g: Group| -> f32 {
            self.prompts
                .iter()
                .zip(&probs)
                .filter(|((group, _), _)| *group == g)
                .map(|(_, p)| p)
                .sum()
        };
        Ok(Scores {
            explicit: hentai + porn,
            suggestive: sexy,
            nudity: sum(Group::Sexual),
            drugs: sum(Group::Drugs),
        })
    }

    /// Juge une image JPEG.
    pub fn judge(&self, jpeg: &[u8]) -> (Verdict, Scores) {
        match decode(jpeg) {
            Some(image) => match self.scores(&image) {
                Ok(scores) => (scores.verdict(), scores),
                Err(e) => {
                    tracing::warn!(target: "alley", "modération : {e}");
                    (Verdict::Unavailable, Scores::default())
                }
            },
            None => (Verdict::Invalid, Scores::default()),
        }
    }
}

/// Une image JPEG (seul format accepté : il ne s'anime pas, ce que voit le
/// serveur est ce que verront les autres).
pub fn decode(bytes: &[u8]) -> Option<image::RgbImage> {
    if !bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return None;
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Jpeg);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(512 << 20);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    (image.width() > 0 && image.height() > 0).then(|| image.to_rgb8())
}

/// La modération, partagée par toutes les requêtes.
#[derive(Clone)]
pub struct Moderation {
    images: Arc<OnceLock<Result<Images, String>>>,
    /// Une image à la fois (le calcul occupe un cœur ~½ s) ; les autres
    /// attendent leur tour.
    gate: Arc<tokio::sync::Semaphore>,
    bans: AutoBan,
    ban_for: Duration,
}

impl Moderation {
    /// Le dossier des modèles : `ALLEY_MODELS`, sinon `models/`.
    pub fn models_dir() -> PathBuf {
        std::env::var_os("ALLEY_MODELS")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("models"))
    }

    pub fn new(bans: AutoBan) -> Moderation {
        let days = std::env::var("ALLEY_BAN_DAYS")
            .ok()
            .and_then(|d| d.parse::<u64>().ok())
            .unwrap_or(30)
            .max(1);
        Moderation {
            images: Arc::new(OnceLock::new()),
            gate: Arc::new(tokio::sync::Semaphore::new(1)),
            bans,
            ban_for: Duration::from_secs(days * 24 * 3600),
        }
    }

    /// Charge les modèles en arrière-plan (une seconde environ).
    pub fn load_in_background(&self) {
        let images = self.images.clone();
        std::thread::spawn(move || {
            let dir = Moderation::models_dir();
            let loaded = Images::load(&dir);
            match &loaded {
                Ok(_) => tracing::info!(target: "alley", "modération des images : prête"),
                Err(e) => tracing::warn!(
                    target: "alley",
                    "modération des images indisponible ({e}) : les images seront refusées"
                ),
            }
            let _ = images.set(loaded);
        });
    }

    /// Les modèles, une fois chargés.
    pub fn images(&self) -> Option<&Images> {
        self.images.get().and_then(|r| r.as_ref().ok())
    }

    /// Juge une image (hors du fil de la boucle asynchrone, une à la fois).
    pub async fn judge(&self, jpeg: Vec<u8>) -> Verdict {
        if self.images.get().is_none() {
            // Le chargement est en cours : quelques secondes au démarrage.
            for _ in 0..120 {
                tokio::time::sleep(Duration::from_millis(250)).await;
                if self.images.get().is_some() {
                    break;
                }
            }
        }
        let Ok(_permit) = self.gate.clone().acquire_owned().await else {
            return Verdict::Unavailable;
        };
        let this = self.clone();
        tokio::task::spawn_blocking(move || match this.images() {
            Some(images) => images.judge(&jpeg).0,
            None => Verdict::Unavailable,
        })
        .await
        .unwrap_or(Verdict::Unavailable)
    }

    pub fn ban(&self, ip: IpAddr) {
        tracing::warn!(target: "alley", %ip, "adresse bloquée (image interdite)");
        self.bans.ban(ip, self.ban_for);
    }

    pub fn is_banned(&self, ip: IpAddr) -> bool {
        self.bans.is_banned(ip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompts_file_matches_the_list() {
        let prompts = load_prompts().unwrap();
        assert_eq!(prompts.len(), PROMPTS.len());
        for (_, v) in &prompts {
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn thresholds() {
        let s = |explicit, suggestive, nudity, drugs| Scores {
            explicit,
            suggestive,
            nudity,
            drugs,
        };
        assert_eq!(
            s(0.9, 0.0, 0.0, 0.0).verdict(),
            Verdict::Banned(Category::Sexual)
        );
        assert_eq!(
            s(0.6, 0.0, 0.7, 0.0).verdict(),
            Verdict::Banned(Category::Sexual)
        );
        assert_eq!(
            s(0.1, 0.6, 0.0, 0.0).verdict(),
            Verdict::Refused(Category::Sexual)
        );
        assert_eq!(
            s(0.0, 0.0, 0.0, 0.8).verdict(),
            Verdict::Banned(Category::Drugs)
        );
        assert_eq!(
            s(0.0, 0.0, 0.0, 0.5).verdict(),
            Verdict::Refused(Category::Drugs)
        );
        assert_eq!(s(0.05, 0.1, 0.1, 0.1).verdict(), Verdict::Clean);
    }

    #[test]
    fn only_jpeg_is_accepted() {
        assert!(decode(b"GIF89a....").is_none());
        assert!(decode(&[0xFF, 0xD8, 0xFF, 0x00]).is_none());
        let mut jpeg = Vec::new();
        image::RgbImage::from_pixel(16, 16, image::Rgb([200, 30, 30]))
            .write_to(
                &mut std::io::Cursor::new(&mut jpeg),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        assert_eq!(decode(&jpeg).unwrap().dimensions(), (16, 16));
    }
}
