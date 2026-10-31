//! Le raccourci global : la seule chose qui declenche une dictee.
//!
//! Deux modes. **Maintien** (le defaut) : on parle tant que la touche est enfoncee, relacher lance
//! la transcription. **Bascule** : un appui ouvre le micro, le suivant le ferme.
//!
//! ⛔ **La bascule a d'abord ete ecartee, puis acceptee le 2026-10-31 a une condition** : un micro
//! ouvert sans rien pour le rappeler. Elle est donc accompagnee d'un rappel visible tant que le
//! micro est ouvert (l'icone rouge, ET la bulle, parce que l'icone est souvent cachee dans la zone
//! de debordement de Windows 11). Retirer ce rappel rouvrirait le defaut d'origine.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::dictee;

/// Ce qu'un evenement du raccourci doit declencher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Commencer,
    Terminer,
    Rien,
}

/// Decide de l'action, sans clavier ni micro : fonction PURE, testee a part.
///
/// - `bascule` : le mode choisi,
/// - `appui` : l'evenement est un appui (sinon un relachement),
/// - `deja_enfoncee` : la touche etait deja enfoncee avant cet appui,
/// - `en_cours` : une dictee est deja en cours.
///
/// ⛔ **Windows repete l'appui tant que la touche est tenue.** En bascule, ces repetitions
/// ouvriraient et fermeraient le micro en boucle : un appui qui arrive alors que la touche est deja
/// enfoncee est une repetition, on l'ignore.
pub fn action(bascule: bool, appui: bool, deja_enfoncee: bool, en_cours: bool) -> Action {
    match (bascule, appui) {
        (false, true) => Action::Commencer,
        (false, false) => Action::Terminer,
        (true, false) => Action::Rien,
        (true, true) if deja_enfoncee => Action::Rien,
        (true, true) if en_cours => Action::Terminer,
        (true, true) => Action::Commencer,
    }
}

/// La touche est-elle tenue en ce moment, d'apres les evenements recus.
static ENFONCEE: AtomicBool = AtomicBool::new(false);

/// Un test du raccourci est-il en cours, depuis l'ecran de reglages.
static EN_TEST: AtomicBool = AtomicBool::new(false);

/// Prend l'evenement pour le test s'il y en a un en cours : rend vrai, et le test est consomme.
///
/// ⛔ **Le raccourci « muet » ne se detecte pas en arriere-plan.** Quand un autre logiciel (jeu,
/// anti-triche, outil de capture ou de securite) pose un crochet clavier global et garde la
/// combinaison pour lui, notre enregistrement a REUSSI et l'evenement n'arrive simplement jamais :
/// rien ne distingue ce cas d'un utilisateur qui n'appuie pas. Seul un test explicite tranche,
/// « appuyez maintenant », d'ou ce mode. Pendant le test, l'appui ne lance PAS de dictee.
///
/// Seul l'APPUI consomme le test : le relachement qui suit est ignore, sinon il serait pris pour
/// la fin d'une dictee jamais commencee.
pub fn prendre_pour_test(appui: bool, drapeau: &AtomicBool) -> bool {
    appui
        && drapeau
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
}

/// Ouvre ou referme la fenetre de test du raccourci.
pub fn tester(actif: bool) {
    EN_TEST.store(actif, Ordering::SeqCst);
}

/// Traduit le libelle des reglages vers ce que le greffon attend.
///
/// ⚠️ **Les deux vocabulaires ne coincident pas** : les reglages ecrivent `Ctrl+Alt+Space`,
/// hérité de l'ancienne version, la ou le greffon veut `CommandOrControl+Alt+Space`. Traduire
/// ici plutot que de changer le format du fichier de configuration, qui est lu par des gens et
/// recopie d'une machine a l'autre.
///
/// Fonction PURE : elle se teste sans enregistrer quoi que ce soit aupres du systeme.
pub fn normaliser(libelle: &str) -> String {
    libelle
        .split('+')
        .map(str::trim)
        .filter(|partie| !partie.is_empty())
        .map(|partie| match partie.to_ascii_lowercase().as_str() {
            // `Ctrl` seul n'est pas compris ; `CommandOrControl` vaut Ctrl ici et Cmd sur macOS,
            // ce qui evite d'ecrire deux reglages pour une seule intention.
            "ctrl" | "control" => "CommandOrControl".to_string(),
            "cmd" | "command" | "super" | "meta" | "win" => "Super".to_string(),
            "alt" | "option" => "Alt".to_string(),
            "shift" | "maj" => "Shift".to_string(),
            // ⚠️ On rend `partie` et NON la version abaissee sur laquelle on a filtre : le
            // greffon attend `Space` et `F9`, pas `space` ni `f9`. Rendre la chaine du `match`
            // transformait silencieusement toutes les touches en minuscules, et aucun raccourci
            // ne s'enregistrait plus.
            _ => partie.to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Compose le libelle d'un raccourci a partir d'une touche pressee dans l'ecran de reglages.
///
/// `code` est le `KeyboardEvent.code` du navigateur (`KeyA`, `Digit1`, `F9`, `Numpad0`,
/// `Insert`...), qui designe la touche PHYSIQUE : le meme raccourci quel que soit l'agencement
/// du clavier, ce que le greffon attend aussi.
///
/// Rend `Ok(None)` tant que seuls des modificateurs sont enfonces (on attend la touche),
/// `Ok(Some(libelle))` pour un raccourci complet et VALIDE, `Err` avec la raison sinon.
///
/// ⛔ **Une touche ordinaire seule est refusee** : « A » ou « Espace » comme raccourci global se
/// declencherait a chaque fois qu'on l'ecrit, dans toutes les applications. Seules les touches
/// qu'on ne tape jamais en ecrivant passent sans modificateur (F1 a F24, Inser, Pause, Arret
/// defil). C'est ce que demandait le backlog de la 1.x.
///
/// ⚠️ Le libelle rendu a deja ete passe par `normaliser` puis par l'analyseur du greffon : un
/// libelle que le systeme refuserait ne sort jamais d'ici.
pub fn composer(
    code: &str,
    ctrl: bool,
    alt: bool,
    maj: bool,
    systeme: bool,
) -> Result<Option<String>, String> {
    const MODIFICATEURS: [&str; 9] = [
        "ControlLeft",
        "ControlRight",
        "AltLeft",
        "AltRight",
        "ShiftLeft",
        "ShiftRight",
        "MetaLeft",
        "MetaRight",
        "OSLeft",
    ];
    if MODIFICATEURS.contains(&code) || code == "OSRight" {
        return Ok(None);
    }

    let touche = if let Some(lettre) = code.strip_prefix("Key") {
        lettre.to_string()
    } else if let Some(chiffre) = code.strip_prefix("Digit") {
        chiffre.to_string()
    } else {
        code.to_string()
    };
    if touche.is_empty() {
        return Err("Touche inconnue.".into());
    }

    let touche_seule_permise = matches!(touche.as_str(), "Insert" | "Pause" | "ScrollLock")
        || touche
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n));
    if !(ctrl || alt || maj || systeme) && !touche_seule_permise {
        return Err(
            "Ajoutez Ctrl, Alt ou Maj : une touche seule se déclencherait à chaque fois que vous \
             l'écrivez."
                .into(),
        );
    }
    // Maj seule avec une lettre ou un chiffre, c'est une majuscule ou un symbole : meme probleme.
    if maj && !(ctrl || alt || systeme) && !touche_seule_permise && touche.chars().count() == 1 {
        return Err(
            "Maj seule avec une lettre ou un chiffre, c'est une majuscule : ajoutez Ctrl ou Alt."
                .into(),
        );
    }

    let mut parties = Vec::new();
    if ctrl {
        parties.push("Ctrl");
    }
    if alt {
        parties.push("Alt");
    }
    if maj {
        parties.push("Shift");
    }
    if systeme {
        parties.push("Super");
    }
    parties.push(&touche);
    let libelle = parties.join("+");

    normaliser(&libelle)
        .parse::<Shortcut>()
        .map_err(|_| format!("Le système ne sait pas utiliser « {libelle} » comme raccourci."))?;
    Ok(Some(libelle))
}

/// Retire le raccourci pendant qu'on en choisit un autre dans les reglages.
///
/// ⚠️ Sans ca, appuyer sur le raccourci ACTUEL pour le garder (ou par megarde) lancerait une
/// dictee au lieu de l'inscrire dans le champ : le systeme intercepte la combinaison avant que
/// la fenetre ne la voie. L'ecran le repose en quittant le champ (`appliquer_raccourci`).
pub fn suspendre(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
    eprintln!("raccourci : suspendu le temps de la saisie");
}

/// Enregistre le raccourci des reglages aupres du systeme.
pub fn installer(app: &AppHandle) -> Result<(), String> {
    let reglages = crate::reglages::lire_sans_application();
    enregistrer(app, &reglages.raccourci)
}

/// Enregistre un raccourci precis, en retirant d'abord ceux qui sont poses.
///
/// ⚠️ Idempotent : appele deux fois avec le meme libelle, il ne laisse qu'un enregistrement.
/// Sans ca, changer de raccourci dans les reglages empilerait l'ancien et le nouveau, et
/// l'ancien continuerait a declencher des dictees sans apparaitre nulle part.
pub fn enregistrer(app: &AppHandle, libelle: &str) -> Result<(), String> {
    let normalise = normaliser(libelle);
    let raccourci: Shortcut = normalise
        .parse()
        .map_err(|_| format!("Le raccourci « {libelle} » n'est pas valide."))?;

    let gestionnaire = app.global_shortcut();
    let _ = gestionnaire.unregister_all();

    // ⚠️ **Une trace a l'enregistrement, et une a chaque declenchement.** Sans elles, un raccourci
    // qui ne repond pas est indistinguable d'un raccourci jamais enregistre : les deux se
    // manifestent par une application parfaitement silencieuse. Constate le 2026-09-21 en
    // eprouvant X11, ou le journal etait vide et ne permettait aucun diagnostic.
    eprintln!("raccourci : « {libelle} » enregistré sous « {normalise} »");

    gestionnaire
        .on_shortcut(raccourci, move |app, _raccourci, evenement| {
            let appui = evenement.state() == ShortcutState::Pressed;
            let deja_enfoncee = ENFONCEE.swap(appui, Ordering::SeqCst) && appui;
            if prendre_pour_test(appui, &EN_TEST) {
                eprintln!("raccourci : reçu pendant le test");
                let _ = tauri::Emitter::emit(app, "raccourci-teste", ());
                return;
            }
            // Relu a chaque evenement : changer de mode dans les reglages vaut tout de suite,
            // sans reenregistrer le raccourci.
            let bascule = crate::reglages::lire_sans_application().mode_raccourci == "bascule";
            match action(bascule, appui, deja_enfoncee, dictee::en_cours(app)) {
                Action::Commencer => {
                    eprintln!("raccourci : début");
                    dictee::commencer(app);
                }
                Action::Terminer => {
                    eprintln!("raccourci : fin");
                    dictee::terminer(app);
                }
                Action::Rien => {}
            }
        })
        .map_err(|erreur| {
        format!(
            "Le raccourci « {libelle} » n'a pas pu être enregistré : il est peut-être déjà pris \
             par une autre application. ({erreur})"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⛔ **Garde l'ecart de vocabulaire entre le fichier de reglages et le greffon.** Les
    /// reglages ecrivent `Ctrl`, que le greffon ne comprend pas : sans traduction, le raccourci
    /// n'est jamais enregistre et la dictee ne se declenche **jamais**, sans message puisque
    /// l'application demarre normalement.
    #[test]
    fn ctrl_devient_le_nom_que_le_greffon_comprend() {
        assert_eq!(normaliser("Ctrl+Alt+Space"), "CommandOrControl+Alt+Space");
        assert_eq!(normaliser("Control+F9"), "CommandOrControl+F9");
    }

    /// ⚠️ Le fichier de reglages est edite a la main : il arrive avec des espaces et des casses
    /// variees, qui ne doivent pas faire echouer l'enregistrement.
    #[test]
    fn la_casse_et_les_espaces_ne_changent_rien() {
        assert_eq!(
            normaliser("ctrl + alt + Space"),
            "CommandOrControl+Alt+Space"
        );
        assert_eq!(normaliser("SHIFT+Insert"), "Shift+Insert");
    }

    /// ⚠️ Les synonymes des touches de commande existent parce que trois systemes les nomment
    /// differemment, et que le meme fichier de reglages voyage de l'un a l'autre.
    #[test]
    fn les_synonymes_des_modificateurs_sont_acceptes() {
        for libelle in ["Cmd+Space", "Command+Space", "Super+Space", "Win+Space"] {
            assert_eq!(normaliser(libelle), "Super+Space", "{libelle}");
        }
        assert_eq!(normaliser("Option+F1"), "Alt+F1");
    }

    /// ⚠️ Une touche inconnue passe telle quelle : c'est au greffon de la refuser, avec son
    /// vocabulaire a lui. La traduire au hasard produirait un raccourci silencieusement different
    /// de celui qui est ecrit dans les reglages.
    #[test]
    fn en_maintien_appui_commence_et_relachement_termine() {
        assert_eq!(action(false, true, false, false), Action::Commencer);
        assert_eq!(action(false, false, true, true), Action::Terminer);
    }

    #[test]
    fn en_bascule_un_appui_ouvre_puis_le_suivant_ferme() {
        assert_eq!(action(true, true, false, false), Action::Commencer);
        assert_eq!(action(true, false, true, true), Action::Rien);
        assert_eq!(action(true, true, false, true), Action::Terminer);
    }

    /// ⛔ Le cas qui justifie `deja_enfoncee` : sans lui, garder la touche un peu longtemps
    /// ouvrirait et fermerait le micro a chaque repetition du clavier.
    #[test]
    fn en_bascule_la_repetition_du_clavier_est_ignoree() {
        assert_eq!(action(true, true, true, true), Action::Rien);
        assert_eq!(action(true, true, true, false), Action::Rien);
    }

    #[test]
    fn une_touche_inconnue_n_est_pas_devinee() {
        assert_eq!(normaliser("Ctrl+Inconnue"), "CommandOrControl+Inconnue");
    }

    #[test]
    fn le_test_prend_un_seul_appui_et_jamais_un_relachement() {
        let drapeau = AtomicBool::new(true);
        assert!(
            !prendre_pour_test(false, &drapeau),
            "un relâchement ne consomme pas le test"
        );
        assert!(
            prendre_pour_test(true, &drapeau),
            "le premier appui est pris pour le test"
        );
        assert!(
            !prendre_pour_test(true, &drapeau),
            "le test est consommé : l'appui suivant redevient une dictée"
        );
    }

    #[test]
    fn hors_test_aucun_appui_n_est_detourne() {
        let drapeau = AtomicBool::new(false);
        assert!(!prendre_pour_test(true, &drapeau));
    }

    #[test]
    fn composer_attend_la_touche_tant_que_seuls_des_modificateurs_sont_enfonces() {
        assert_eq!(composer("ControlLeft", true, false, false, false), Ok(None));
        assert_eq!(composer("AltRight", true, true, false, false), Ok(None));
    }

    #[test]
    fn composer_ecrit_le_libelle_dans_le_vocabulaire_des_reglages() {
        assert_eq!(
            composer("Space", true, true, false, false),
            Ok(Some("Ctrl+Alt+Space".into()))
        );
        assert_eq!(
            composer("KeyD", true, false, true, false),
            Ok(Some("Ctrl+Shift+D".into()))
        );
        assert_eq!(
            composer("Digit5", false, true, false, false),
            Ok(Some("Alt+5".into()))
        );
        assert_eq!(
            composer("Numpad0", true, false, false, false),
            Ok(Some("Ctrl+Numpad0".into()))
        );
    }

    #[test]
    fn composer_accepte_seules_les_touches_qu_on_ne_tape_jamais_en_ecrivant() {
        assert_eq!(
            composer("F9", false, false, false, false),
            Ok(Some("F9".into()))
        );
        assert_eq!(
            composer("F24", false, false, false, false),
            Ok(Some("F24".into()))
        );
        assert_eq!(
            composer("Insert", false, false, false, false),
            Ok(Some("Insert".into()))
        );
        assert!(composer("KeyA", false, false, false, false).is_err());
        assert!(composer("Space", false, false, false, false).is_err());
        assert!(composer("Home", false, false, false, false).is_err());
    }

    #[test]
    fn composer_refuse_maj_seule_avec_une_lettre_qui_ferait_une_majuscule() {
        assert!(composer("KeyA", false, false, true, false).is_err());
        assert!(composer("Digit1", false, false, true, false).is_err());
        // Maj avec une touche qui n'ecrit rien reste permise.
        assert_eq!(
            composer("F5", false, false, true, false),
            Ok(Some("Shift+F5".into()))
        );
    }

    #[test]
    fn composer_rend_un_libelle_que_le_greffon_accepte_a_la_relecture() {
        // Ce que `composer` rend est exactement ce qu'`enregistrer` relira depuis le fichier.
        for (code, ctrl, alt) in [
            ("Space", true, true),
            ("KeyQ", false, true),
            ("F12", true, false),
        ] {
            let libelle = composer(code, ctrl, alt, false, false).unwrap().unwrap();
            assert!(
                normaliser(&libelle).parse::<Shortcut>().is_ok(),
                "{libelle}"
            );
        }
    }
}
