//! Oyant, dictee vocale locale et multiplateforme.
//!
//! Etape 1 : la fenetre existe, sans barre de titre. Elle ne dicte rien encore.
//!
//! ⚠️ **Pas de `windows_subsystem = "windows"`, et c'est un arbitrage, pas un oubli.**
//! Ce drapeau supprime la console attachee sur Windows, ce qui evite qu'une fenetre noire
//! clignote au lancement depuis la zone de notification. Mais il **coupe aussi la sortie
//! standard**, donc `oyant.exe --version` n'afficherait plus rien : le workflow de publication,
//! qui compare cette sortie au tag, echouerait, et le mode ligne de commande de l'inventaire
//! serait muet. Le vrai correctif est d'attacher la console du parent quand des arguments sont
//! presents, ce qui appartient a l'etape 8. D'ici la, on garde une sortie qui fonctionne.

mod appairage;
mod audio;
mod barre;
mod bulle;
mod chemins;
mod dictee;
mod historique;
mod injection;
mod medias;
mod modeles;
mod moteur;
mod platform;
mod raccourci;
mod reglages;
mod reseau;
mod serveur;
mod texte;
mod vad;

use tauri::Manager;

use serde::Serialize;

/// Version DERIVEE par `build.rs` depuis le tag git, jamais lue dans le manifeste.
const VERSION: &str = env!("OYANT_VERSION");

/// Ce que la ligne de commande demande.
///
/// Fonction de decision separee de l'execution pour qu'elle soit testable sans lancer de
/// processus ni capturer de sortie standard.
// Pas de `Copy` : la variante `Inconnu` porte une `String`, qui possede son contenu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Demande {
    /// La version seule, sans rien autour.
    ///
    /// ⚠️ « Sans rien autour » est une exigence et pas une preference : le workflow de
    /// publication compare cette sortie au tag, caractere pour caractere, et refuse de publier un
    /// binaire qui ne s'annonce pas comme le tag qui l'a construit.
    Version,
    Aide,
    /// Aucun argument : on ouvre l'interface.
    Interface,
    /// Lance par le demarrage du systeme : l'application se met dans la zone de notification
    /// SANS ouvrir sa fenetre.
    ///
    /// ⚠️ Ouvrir la fenetre a chaque ouverture de session serait la meilleure facon de faire
    /// desactiver le demarrage automatique par l'utilisateur. Une application qui se lance avec
    /// le systeme doit se faire oublier.
    AuDemarrage,
    /// Transcrire un fichier audio et rendre son texte.
    ///
    /// ⚠️ **La ligne de commande vient AVANT l'interface de dictee**, et c'est l'ordre voulu :
    /// elle se teste sans micro, sans raccourci global et sans injection de frappe, donc elle
    /// valide le moteur avant qu'on y ajoute trois difficultes d'un coup.
    Transcrire(Demandetranscription),
    /// Argument non reconnu, rendu tel quel pour pouvoir le citer dans le message d'erreur.
    Inconnu(String),
}

/// Les options d'une transcription en ligne de commande.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Demandetranscription {
    fichier: String,
    modele: Option<String>,
    langue: Option<String>,
    sortie: Option<String>,
    silencieux: bool,
}

fn interpreter(arguments: &[String]) -> Demande {
    match arguments.first().map(String::as_str) {
        None => Demande::Interface,
        Some("--version" | "-V") => Demande::Version,
        Some("--help" | "-h") => Demande::Aide,
        // Argument pose par le plugin de demarrage automatique, jamais tape a la main.
        Some("--autostart") => Demande::AuDemarrage,
        // Tout ce qui ne commence pas par un tiret est un fichier a transcrire.
        Some(premier) if !premier.starts_with('-') => transcription(arguments),
        Some(autre) => Demande::Inconnu(autre.to_string()),
    }
}

/// Analyse `oyant FICHIER [options]`.
///
/// ⚠️ Une option qui attend une valeur et n'en trouve pas est **refusee** plutot que silencieuse.
/// `oyant a.wav --model` sans valeur qui prendrait un defaut ferait transcrire avec un modele
/// que l'utilisateur n'a pas demande, sans que rien ne le dise.
fn transcription(arguments: &[String]) -> Demande {
    let mut demande = Demandetranscription {
        fichier: arguments[0].clone(),
        ..Default::default()
    };

    let mut index = 1;
    while index < arguments.len() {
        let option = arguments[index].as_str();
        let mut valeur = || {
            index += 1;
            arguments.get(index).cloned()
        };

        match option {
            "-m" | "--model" => match valeur() {
                Some(v) => demande.modele = Some(v),
                None => return Demande::Inconnu("--model (missing value)".to_string()),
            },
            "-l" | "--language" => match valeur() {
                Some(v) => demande.langue = Some(v),
                None => return Demande::Inconnu("--language (missing value)".to_string()),
            },
            "-o" | "--output" => match valeur() {
                Some(v) => demande.sortie = Some(v),
                None => return Demande::Inconnu("--output (missing value)".to_string()),
            },
            "-q" | "--quiet" => demande.silencieux = true,
            autre => return Demande::Inconnu(autre.to_string()),
        }
        index += 1;
    }

    Demande::Transcrire(demande)
}

/// Ce que l'interface affiche de la plateforme.
#[derive(Serialize)]
struct EtatPlateforme {
    version: String,
    plateforme: String,
    injection: String,
}

#[tauri::command]
fn etat_plateforme() -> EtatPlateforme {
    EtatPlateforme {
        version: VERSION.to_string(),
        plateforme: format!("{:?}", platform::famille_courante()),
        injection: format!("{:?}", platform::strategie_courante()),
    }
}

/// Les microphones disponibles, pour l'ecran de reglages.
#[tauri::command]
fn microphones() -> Vec<String> {
    audio::microphones()
}

/// Reenregistre le raccourci apres une modification des reglages.
///
/// ⚠️ Sans cette commande, changer le raccourci dans l'ecran n'aurait d'effet **qu'au prochain
/// lancement**, ce qui se lit comme « le reglage ne marche pas ».
#[tauri::command]
fn appliquer_raccourci(app: tauri::AppHandle, libelle: String) -> Result<(), String> {
    raccourci::enregistrer(&app, &libelle)
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let demande = interpreter(&arguments);

    // ⛔ **Un seul appel, ici, et pas dans chaque branche.** L'interface et la ligne de commande
    // lisent toutes les deux le repertoire de donnees ; brancher la reprise cote par cote ferait
    // qu'une troisieme entree, un jour, l'oublierait. Pose au point de passage commun, elle ne
    // peut plus l'etre par personne.
    //
    // ⚠️ Ecarte pour `--version` et `--help`, qui ne lisent rien et dont la sortie est comparee
    // au tag par le workflow de publication, caractere pour caractere.
    if !matches!(demande, Demande::Version | Demande::Aide) {
        for fait in chemins::migrer_ancien_nom() {
            eprintln!("{fait}");
        }
    }

    match demande {
        Demande::Version => println!("{VERSION}"),
        Demande::Aide => aide(),
        Demande::Interface => interface(true),
        Demande::AuDemarrage => interface(false),
        Demande::Transcrire(demande) => {
            if let Err(message) = transcrire(&demande) {
                eprintln!("{message}");
                std::process::exit(1);
            }
        }
        Demande::Inconnu(argument) => {
            // Sur la sortie d'erreur et avec un code non nul : un argument mal ecrit dans un
            // script doit se voir, pas passer pour un succes.
            eprintln!("Unknown argument: {argument}");
            eprintln!("Try `oyant --help`.");
            std::process::exit(2);
        }
    }
}

fn aide() {
    println!("Oyant {VERSION}");
    println!();
    println!("Usage: oyant [FILE] [OPTIONS]");
    println!();
    println!("  With no argument, opens the interface.");
    println!("  With a FILE, transcribes it and prints the text.");
    println!("  Supported audio formats: wav, mp3, ogg, flac.");
    println!();
    // ⚠️ Derivee du catalogue : ecrite a la main, cette liste a oublie Parakeet le jour meme
    // de son ajout.
    let identifiants: Vec<&str> = modeles::CATALOGUE.iter().map(|m| m.identifiant).collect();
    println!(
        "  -m, --model ID      Model to use ({})",
        identifiants.join(", ")
    );
    println!("  -l, --language CODE Spoken language, or `auto` to detect it");
    println!("  -o, --output FILE   Write the text to a file instead of standard output");
    println!("  -q, --quiet         Print the text alone, without the summary line");
    println!("  -V, --version       Print the version and exit");
    println!("      --autostart     Start in the notification area without showing the window");
    println!("  -h, --help          Print this help and exit");
}

/// Transcrit un fichier depuis la ligne de commande.
///
/// ⚠️ **Les reglages du fichier de configuration servent de defauts**, et les options de la ligne
/// de commande les remplacent. Sans ca, `oyant fichier.wav` se comporterait autrement que
/// l'interface sur la meme machine, ce qui rendrait tout diagnostic impossible.
///
/// ⛔ **Chaque cause d'echec est nommee precisement.** « Le moteur n'est pas installe » et « le
/// modele n'est pas telecharge » demandent deux gestes differents ; un message unique du genre
/// « transcription impossible » obligerait a deviner lequel.
fn transcrire(demande: &Demandetranscription) -> Result<(), String> {
    let reglages = reglages::lire_sans_application();

    let modele_id = demande
        .modele
        .clone()
        .unwrap_or_else(|| reglages.modele.clone());

    // ⛔ **Une seule resolution pour la ligne de commande et pour la dictee.** Ce calcul a deja
    // existe en double, et la copie d'ici ignorait la branche « moteur embarque » : elle annoncait
    // « le moteur n'est pas installe » sur un moteur livre avec le produit. `dictee::resoudre`
    // rend une cause TYPEE, que chacun rend dans sa langue, la ligne de commande parlant anglais.
    // ⚠️ `clone()` et pas un deplacement : les reglages servent encore plus bas, pour deduire le
    // vocabulaire a donner au moteur.
    let langue = demande
        .langue
        .clone()
        .unwrap_or_else(|| reglages.langue.clone());
    let outils = dictee::resoudre(&modele_id, &langue).map_err(|manque| manque.en_anglais())?;
    let audio = std::path::PathBuf::from(&demande.fichier);

    let prompt = moteur::prompt_des_reglages(&reglages);
    // La ligne de commande parle anglais : le message d'origine, en francais, est mis en annexe.
    let vad = vad::selon(&reglages)
        .map_err(|erreur| format!("Voice detection model could not be written ({erreur})."))?;
    let resultat = moteur::transcrire(
        &outils.executable,
        &moteur::Demande {
            famille: outils.famille,
            modele: &outils.modele,
            audio: &audio,
            langue: &langue,
            fils: reglages.fils,
            temperature: reglages.temperature,
            prompt: prompt.as_deref(),
            vad: vad.as_deref(),
        },
    )?;

    match &demande.sortie {
        Some(fichier) => {
            std::fs::write(fichier, &resultat.texte)
                .map_err(|erreur| format!("Could not write {fichier}: {erreur}"))?;
            if !demande.silencieux {
                eprintln!(
                    "{} characters written to {fichier} in {:.1}s",
                    resultat.texte.chars().count(),
                    resultat.secondes
                );
            }
        }
        None => {
            // Le texte sur la SORTIE STANDARD, le resume sur la sortie d'erreur : `oyant a.wav >
            // t.txt` doit donner le texte seul, pas le texte suivi d'une ligne de statistiques.
            println!("{}", resultat.texte);
            if !demande.silencieux {
                eprintln!(
                    "Transcribed with `{modele_id}` in {:.1}s",
                    resultat.secondes
                );
            }
        }
    }

    Ok(())
}

fn interface(fenetre_visible: bool) {
    // ⛔ Garde-fou contre un piege deja paye DEUX FOIS dans le parc (voir docs/beammeup.md) :
    // compiler avec `cargo build` nu au lieu de `npm run tauri build` n'active pas la
    // fonctionnalite `custom-protocol`, et le binaire va alors chercher le serveur de
    // developpement sur localhost au lieu des fichiers embarques. La fenetre s'ouvre, la ligne de
    // commande marche, l'IPC marche, et pourtant l'utilisateur ne voit qu'un
    // « localhost refused to connect ». Rien dans la compilation ne le signale.
    //
    // En release, ce cas ne doit jamais atteindre l'utilisateur : on echoue tout de suite et on
    // dit quoi faire.
    #[cfg(not(debug_assertions))]
    if tauri::is_dev() {
        eprintln!("This binary was built without `custom-protocol`: it would load the interface");
        eprintln!("from localhost instead of the embedded files.");
        eprintln!("Build with `npm run tauri build`, never with `cargo build` alone.");
        std::process::exit(3);
    }

    tauri::Builder::default()
        // L'argument passe ici est celui que le systeme ajoutera a la ligne de commande au
        // demarrage de session : c'est lui qui nous dit de ne pas ouvrir la fenetre.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .invoke_handler(tauri::generate_handler![
            etat_plateforme,
            reglages::demarrage_automatique,
            reglages::definir_demarrage_automatique,
            reglages::lire_reglages,
            reglages::ecrire_reglages,
            reglages::chemin_reglages,
            reglages::reglages_par_defaut,
            serveur::reseau_empreinte,
            serveur::reseau_appareils,
            serveur::reseau_revoquer,
            serveur::reseau_repondre,
            modeles::etat_modeles,
            modeles::contexte_mesures,
            modeles::telecharger_modele,
            modeles::verifier_modele,
            modeles::supprimer_modele,
            modeles::interrompre_modele,
            modeles::choisir_modele,
            moteur::etat_moteurs,
            moteur::installer_moteur,
            moteur::choisir_moteur,
            moteur::prechauffer,
            microphones,
            appliquer_raccourci,
            historique::etat_historique,
            historique::effacer_historique
        ])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // ⚠️ Le presse-papiers est le FILET de l'injection : quand la fenêtre visée ne revient
        // pas au premier plan, le texte y atterrit au lieu d'être tapé au hasard.
        .plugin(tauri_plugin_clipboard_manager::init())
        // ⚠️ Le SECOND canal des erreurs. La fenetre est fermee la plupart du temps, donc un
        // evenement seul ne remonte rien : une dictee ratee serait silencieuse.
        .plugin(tauri_plugin_notification::init())
        .manage(modeles::Interruptions::default())
        .manage(dictee::EnCours::default())
        .setup(move |app| {
            barre::installer(app.handle())?;

            // ⚠️ Un raccourci deja pris par une autre application ne doit pas empecher Oyant de
            // demarrer : le reste du produit fonctionne, et c'est dans les reglages qu'on en
            // changera. On le dit, on continue.
            if let Err(message) = raccourci::installer(app.handle()) {
                eprintln!("raccourci global : {message}");
            }

            // ⛔ Ne lie AUCUN port tant que l'utilisateur ne l'a pas demandé : le filtre est dans
            // `demarrer_si_demande`, au démarrage, et pas plus loin. Un serveur qu'on ouvre puis
            // qu'on « protège » ensuite est un serveur ouvert.
            serveur::demarrer_si_demande(app.handle());

            if !fenetre_visible && let Some(fenetre) = app.get_webview_window("main") {
                let _ = fenetre.hide();
            }
            Ok(())
        })
        .on_window_event(|fenetre, evenement| {
            // ⚠️ Fermer la fenetre MASQUE au lieu de quitter. Oyant vit dans la zone de
            // notification : arreter le programme parce que la fenetre se ferme couperait le
            // raccourci global alors que l'icone serait toujours la. Seul « Quitter » du menu
            // de l'icone arrete reellement l'application.
            if let tauri::WindowEvent::CloseRequested { api, .. } = evenement {
                api.prevent_close();
                let _ = fenetre.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("l'interface n'a pas pu demarrer");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(liste: &[&str]) -> Vec<String> {
        liste.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sans_argument_on_ouvre_l_interface() {
        assert_eq!(interpreter(&args(&[])), Demande::Interface);
    }

    #[test]
    fn les_deux_ecritures_de_version_sont_acceptees() {
        assert_eq!(interpreter(&args(&["--version"])), Demande::Version);
        assert_eq!(interpreter(&args(&["-V"])), Demande::Version);
    }

    #[test]
    fn les_deux_ecritures_d_aide_sont_acceptees() {
        assert_eq!(interpreter(&args(&["--help"])), Demande::Aide);
        assert_eq!(interpreter(&args(&["-h"])), Demande::Aide);
    }

    /// ⚠️ Cet argument est pose par le systeme au demarrage de session. S'il cessait d'etre
    /// reconnu, Oyant sortirait en erreur a chaque ouverture de session **sans que personne ne
    /// voie le message**, et le demarrage automatique paraitrait simplement ne pas marcher.
    #[test]
    fn l_argument_de_demarrage_automatique_est_reconnu() {
        assert_eq!(interpreter(&args(&["--autostart"])), Demande::AuDemarrage);
    }

    /// ⛔ **La surface de ligne de commande est en ANGLAIS**, l'interface graphique reste en
    /// francais. C'est la CLI qu'un public non francophone rencontrera en premier, et renommer un
    /// drapeau apres publication serait une rupture : celui du demarrage automatique est ecrit
    /// dans la cle `Run` du systeme, donc un ancien nom encore inscrit ferait sortir Oyant en
    /// erreur a chaque ouverture de session, sans que personne ne voie le message.
    ///
    /// Ce test echoue donc si un drapeau accentue ou francais reapparait.
    #[test]
    fn les_drapeaux_sont_en_anglais() {
        // Tout ce que `interpreter` reconnait, hors la variante « aucun argument ».
        let drapeaux = ["--version", "-V", "--help", "-h", "--autostart"];
        for drapeau in drapeaux {
            assert!(
                drapeau.is_ascii(),
                "{drapeau} : un drapeau ne doit porter aucun caractere accentue"
            );
            assert_ne!(
                interpreter(&args(&[drapeau])),
                Demande::Inconnu(drapeau.to_string()),
                "{drapeau} n'est plus reconnu"
            );
        }

        // Et les anciens noms francais ne doivent plus rien declencher.
        for ancien in ["--au-demarrage", "--aide", "--version-fr"] {
            assert_eq!(
                interpreter(&args(&[ancien])),
                Demande::Inconnu(ancien.to_string()),
                "{ancien} : un nom francais ne doit pas etre reconnu"
            );
        }
    }

    #[test]
    fn un_argument_inconnu_est_refuse_et_cite() {
        assert_eq!(
            interpreter(&args(&["--gpu-layers"])),
            Demande::Inconnu("--gpu-layers".to_string())
        );
    }

    /// ⚠️ La version derivee ne doit jamais porter le `v` du tag : le workflow de publication
    /// compare la sortie de `--version` au nom du tag prive de son `v`, a l'identique.
    #[test]
    fn la_version_ne_porte_pas_le_v_du_tag() {
        assert!(!VERSION.starts_with('v'), "version obtenue : {VERSION}");
        assert!(!VERSION.is_empty());
    }
}
