//! Démarre Alley sur `PORT` (fourni par Heroku), 3000 sinon. Aucun message
//! n'est enregistré : les salons vivent en mémoire. Aucune base externe non
//! plus : seulement un fichier SQLite local (`ALLEY_SQLITE`, `alley.db` par
//! défaut), même si `DATABASE_URL` est réglé ; le reste est dans le
//! `localStorage` des navigateurs et chez Stripe.
//!
//! - `alley export <dossier>` écrit la version GitHub Pages (voir
//!   `alley::export`) ;
//! - `alley judge <image.jpg>…` montre ce que la modération voit dans des
//!   images (voir `alley::moderation`).

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("export") => {
            let dir = std::path::PathBuf::from(args.get(2).map_or("dist", String::as_str));
            let site = alley::export::site_from_env();
            tokio::runtime::Runtime::new()?.block_on(alley::export::run(&dir, &site))
        }
        Some("judge") => {
            let dir = alley::moderation::Moderation::models_dir();
            let images = alley::moderation::Images::load(&dir).map_err(std::io::Error::other)?;
            for path in &args[2..] {
                let bytes = std::fs::read(path)?;
                let (verdict, s) = images.judge(&bytes);
                println!(
                    "{path} : {verdict:?} · explicite {:.2} · suggestif {:.2} · nudité {:.2} · drogue {:.2}",
                    s.explicit, s.suggestive, s.nudity, s.drugs
                );
            }
            Ok(())
        }
        _ => {
            println!("Alley 🔒 écrit avec Paquets 📦, chiffré avec Brocolis 🥦");
            let path = std::env::var("ALLEY_SQLITE").unwrap_or_else(|_| "alley.db".into());
            let mut app = alley::app();
            app.database(format!("sqlite://{path}"));
            app.run()
        }
    }
}
