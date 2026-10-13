//! Démarre Alley sur `PORT` (fourni par Heroku), 3000 sinon.
//!
//! `DATABASE_URL` choisit où vivent les données (toutes chiffrées) : en
//! mémoire par défaut, `memory://alley.memoire` pour les garder dans un
//! fichier, `sqlite://alley.db`, ou `postgres://…` (Heroku Postgres).

fn main() -> std::io::Result<()> {
    println!("Alley 🔒 écrit avec Paquets 📦, chiffré avec Brocolis 🥦");
    alley::app().run()
}
