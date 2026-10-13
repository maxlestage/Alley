//! Démarre Alley sur `PORT` (fourni par Heroku), 3000 sinon.
//!
//! `DATABASE_URL` choisit où vivent les données (toutes chiffrées) : en
//! mémoire par défaut, `memory://alley.memoire` pour les garder dans un
//! fichier, `sqlite://alley.db`, ou `postgres://…` (Heroku Postgres).
//!
//! `alley export <dossier>` écrit la version GitHub Pages (voir
//! `alley::export`).

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("export") {
        let dir = std::path::PathBuf::from(args.get(2).map_or("dist", String::as_str));
        let site = alley::export::site_from_env();
        return tokio::runtime::Runtime::new()?.block_on(alley::export::run(&dir, &site));
    }
    println!("Alley 🔒 écrit avec Paquets 📦, chiffré avec Brocolis 🥦");
    alley::app().run()
}
