//! La dictee elle-meme : on tient le raccourci, on parle, ca s'ecrit.
//!
//! Ce module assemble les trois briques de l'etape 7 (capture, raccourci global, injection) et
//! ne contient presque aucune logique propre. Ce qu'il porte vraiment, c'est l'ORDRE des choses
//! et ce qu'on fait quand l'une d'elles echoue.
//!
//! ⛔ **Le raccourci se TIENT, il ne se presse pas.** C'est une decision de produit heritee de
//! l'ancienne version : on parle tant que la touche est enfoncee, et relacher declenche la
//! transcription. Un raccourci a bascule laisserait un microphone ouvert sans que rien ne le
//! rappelle, sur une application dont la fenetre n'est meme pas visible.

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::barre::{self, Etat};
use crate::{audio, chemins, historique, injection, medias, modeles, moteur, reglages, texte};

/// Ce qu'il manque pour pouvoir transcrire.
///
/// ⛔ **Typee, et pas une chaine de caracteres.** Deux raisons, et la seconde n'est pas evidente :
/// « le modele n'est pas telecharge » et « le moteur n'est pas installe » demandent deux gestes
/// differents, donc un message unique obligerait a deviner lequel ; et la ligne de commande parle
/// **anglais** quand l'interface parle **francais**, si bien qu'une chaine construite ici serait
/// forcement fausse pour l'un des deux appelants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Manque {
    ModeleInconnu(String),
    ModeleAbsent(String),
    AucunMoteur,
    MoteurAbsent,
    /// Le modele choisi ne sait pas transcrire la langue choisie (Parakeet et le coreen).
    LangueRefusee {
        modele: String,
        langue: String,
    },
    /// Le moteur choisi ne contient pas le programme qui lit ce modele.
    FamilleIndisponible {
        modele: String,
        moteur: String,
    },
}

/// De quoi lancer une transcription.
pub struct Outils {
    pub executable: PathBuf,
    pub modele: PathBuf,
    pub famille: modeles::Famille,
}

/// Trouve le moteur et le modele, ou dit precisement lequel manque.
///
/// ⛔ **Un seul chemin pour l'interface ET la ligne de commande.** Ce calcul existait en double,
/// et la copie de la ligne de commande ignorait la branche « moteur embarque » : elle annoncait
/// « le moteur n'est pas installe » sur un moteur livre avec le produit. Deux copies d'une regle
/// finissent toujours par diverger, et celle qu'on ne regarde pas est celle qui ment.
///
/// ⚠️ **La langue est verifiee AVANT tout le reste**, avant meme de savoir si le modele est
/// telecharge : sinon quelqu'un qui dicte en coreen avec Parakeet se ferait d'abord dire de
/// telecharger 669 Mo, pour apprendre ensuite que ca ne servirait a rien.
pub fn resoudre(modele_id: &str, langue: &str) -> Result<Outils, Manque> {
    let modele = modeles::par_identifiant(modele_id)
        .ok_or_else(|| Manque::ModeleInconnu(modele_id.to_string()))?;

    if !modele.famille.connait(langue) {
        return Err(Manque::LangueRefusee {
            modele: modele.nom.to_string(),
            langue: langue.to_string(),
        });
    }

    let chemin_modele = chemins::modeles()
        .map_err(|_| Manque::ModeleAbsent(modele_id.to_string()))?
        .join(modele.fichier);
    if !chemin_modele.is_file() {
        return Err(Manque::ModeleAbsent(modele_id.to_string()));
    }

    let moteur = moteur::actif().ok_or(Manque::AucunMoteur)?;
    let present = |chemin: Result<PathBuf, String>| chemin.ok().filter(|c| c.is_file());
    let mut executable = present(moteur::executable_pour(moteur, modele.famille));

    // ⚠️ **Parakeet passe par le moteur EMBARQUE quand le moteur choisi ne l'a pas SUR LE
    // DISQUE**, et pas seulement quand le catalogue dit qu'il ne l'a pas. Un moteur Vulkan
    // installe avant que son archive ne contienne Parakeet est declare capable et ne l'est pas :
    // decider sur le catalogue aurait annonce « programme non installe » a quelqu'un dont la
    // dictee Whisper marche parfaitement. Refuser aurait de toute facon bloque precisement ceux
    // qui ont choisi une carte, alors que Parakeet est concu pour le processeur. Ce n'est pas un
    // repli cache : la description du modele dit qu'il calcule sur le processeur.
    if executable.is_none() && modele.famille == modeles::Famille::Parakeet {
        match moteur::MOTEURS
            .iter()
            .find(|m| m.embarque && !m.executable_parakeet.is_empty())
        {
            Some(embarque) => {
                executable = present(moteur::executable_pour(embarque, modele.famille));
            }
            None if moteur.executable_parakeet.is_empty() => {
                return Err(Manque::FamilleIndisponible {
                    modele: modele.nom.to_string(),
                    moteur: moteur.nom.to_string(),
                });
            }
            None => {}
        }
    }
    let executable = executable.ok_or(Manque::MoteurAbsent)?;

    Ok(Outils {
        executable,
        modele: chemin_modele,
        famille: modele.famille,
    })
}

impl Manque {
    /// Le message de l'interface, en francais, avec le geste a faire.
    pub fn en_francais(&self) -> String {
        match self {
            Manque::ModeleInconnu(id) => {
                format!("Le modèle « {id} » n'existe pas. Choisissez-en un dans les réglages.")
            }
            Manque::ModeleAbsent(id) => format!(
                "Le modèle « {id} » n'est pas téléchargé. Ouvrez les réglages pour l'installer."
            ),
            Manque::AucunMoteur => {
                "Aucun programme de transcription n'est disponible sur ce système.".to_string()
            }
            Manque::MoteurAbsent => {
                "Le programme de transcription n'est pas installé. Ouvrez les réglages pour \
                 l'installer."
                    .to_string()
            }
            // ⚠️ Nomme le remede, qui est de changer de MODELE : « langue non prise en charge »
            // seul ferait chercher dans le reglage de langue.
            Manque::LangueRefusee { modele, langue } => format!(
                "{modele} ne transcrit pas cette langue ({langue}). Choisissez un modèle Whisper, \
                 ou la détection automatique."
            ),
            Manque::FamilleIndisponible { modele, moteur } => format!(
                "{modele} ne fonctionne pas encore avec « {moteur} ». Choisissez « Le processeur » \
                 dans les réglages, ou un modèle Whisper."
            ),
        }
    }

    /// Le message de la ligne de commande, en anglais.
    pub fn en_anglais(&self) -> String {
        match self {
            Manque::ModeleInconnu(id) => format!("Unknown model: {id}"),
            Manque::ModeleAbsent(id) => format!(
                "Model `{id}` is not downloaded.\nOpen Oyant and download it from the Models section."
            ),
            Manque::AucunMoteur => {
                "No transcription engine is available for this platform yet.".to_string()
            }
            Manque::MoteurAbsent => {
                "The transcription engine is not installed.\nOpen Oyant and install it from the \
                 Models section."
                    .to_string()
            }
            Manque::LangueRefusee { modele, langue } => format!(
                "{modele} cannot transcribe language `{langue}`.\nUse a Whisper model, or `-l auto`."
            ),
            Manque::FamilleIndisponible { modele, moteur } => format!(
                "{modele} does not run on the engine \"{moteur}\" yet.\nSelect the CPU engine in \
                 Oyant, or a Whisper model."
            ),
        }
    }
}

/// Une dictee en cours.
pub struct Session {
    enregistrement: audio::Enregistrement,
    /// La fenetre ou le texte doit atterrir, quand c'est le menu qui a lance la dictee.
    ///
    /// ⛔ **Vide pour le raccourci global, et c'est delibere.** Le raccourci ne touche pas au
    /// focus, donc il n'y a rien a restaurer ; forcer un retour ecraserait au contraire un
    /// changement de fenetre fait expres pendant qu'on parle. Le menu, lui, prend le focus par
    /// construction : sans cette memoire, le texte irait dans la fenetre d’Oyant.
    cible: Option<isize>,
    /// La musique mise en pause pour cette dictee, relancee quand la valeur est abandonnee.
    musique: medias::Reprise,
}

/// L'enregistrement en cours, s'il y en a un.
#[derive(Default)]
pub struct EnCours(pub Mutex<Option<Session>>);

/// Y a-t-il une dictee en cours ? Sert au menu pour dire ce que son entree va faire.
pub fn en_cours(app: &AppHandle) -> bool {
    app.state::<EnCours>()
        .0
        .lock()
        .map(|place| place.is_some())
        .unwrap_or(false)
}

/// Demarre ou arrete une dictee, selon l'etat courant.
///
/// ⚠️ **Le menu de l'icone ne peut pas se tenir enfonce**, donc il bascule, contrairement au
/// raccourci. Ce qui rendait la bascule dangereuse (un microphone ouvert sans rien pour le
/// rappeler) est couvert par deux choses qui existent maintenant : l'icone change d'etat, et la
/// duree maximale coupe l'enregistrement toute seule.
pub fn basculer(app: &AppHandle) {
    if en_cours(app) {
        terminer(app);
    } else {
        commencer_depuis_le_menu(app);
    }
}

/// Lance une dictee depuis le menu, en retenant la fenetre a laquelle rendre le focus.
pub fn commencer_depuis_le_menu(app: &AppHandle) {
    let cible = injection::fenetre_cible();
    commencer_avec(app, cible);
}

/// Le raccourci vient d'etre enfonce : on ouvre le microphone.
pub fn commencer(app: &AppHandle) {
    commencer_avec(app, None);
}

fn commencer_avec(app: &AppHandle, cible: Option<isize>) {
    let reglages = reglages::lire_sans_application();

    // ⚠️ On verifie le moteur et le modele AVANT d'ouvrir le microphone. Enregistrer d'abord
    // ferait parler quelqu'un dans le vide pour lui annoncer ensuite qu'il manquait un fichier.
    if let Err(manque) = resoudre(&reglages.modele, &reglages.langue) {
        signaler(app, &manque.en_francais());
        return;
    }

    let etat = app.state::<EnCours>();
    let Ok(mut place) = etat.0.lock() else {
        return;
    };
    // Deja en train d'enregistrer : le systeme repete l'appui tant que la touche est tenue.
    if place.is_some() {
        return;
    }

    // La musique se coupe AVANT le bip, pour que le bip s'entende et que le microphone ne
    // capte pas la chanson. Reglage desactive : une reprise vide, qui ne relance rien.
    let musique = if reglages.pause_medias {
        medias::suspendre()
    } else {
        medias::Reprise::default()
    };

    // ⛔ Le bip AVANT d'ouvrir le microphone, jamais pendant. Joue en parallele, Oyant
    // s'enregistrerait lui-meme, et un bip capte depasse le seuil de silence : un appui
    // accidentel declencherait alors une transcription au lieu d'etre reconnu comme
    // « personne n'a parle ».
    if reglages.bip_debut
        && let Err(message) = audio::bip(reglages.frequence_bip_hz, reglages.duree_bip_ms)
    {
        // Un bip muet ne doit pas empecher de dicter : on le note, on continue.
        eprintln!("bip de début : {message}");
    }

    match audio::demarrer(&reglages.microphone, reglages.duree_maximale_s) {
        Ok(enregistrement) => {
            *place = Some(Session {
                enregistrement,
                cible,
                musique,
            });
            let _ = barre::definir_etat(app, Etat::Enregistrement);
            let _ = barre::rafraichir_entree_dictee(app, true);
        }
        Err(message) => signaler(app, &message),
    }
}

/// Le raccourci vient d'etre relache : on transcrit, puis on ecrit.
pub fn terminer(app: &AppHandle) {
    let etat = app.state::<EnCours>();
    let Ok(mut place) = etat.0.lock() else {
        return;
    };
    let Some(session) = place.take() else {
        return;
    };
    drop(place);
    let _ = barre::rafraichir_entree_dictee(app, false);

    let reglages = reglages::lire_sans_application();
    let duree_ms = session.enregistrement.duree_ms();
    let cible = session.cible;
    let echantillons = session.enregistrement.arreter();
    // ⚠️ La musique repart DES que le microphone est coupe, sans attendre la transcription,
    // qui peut prendre plusieurs secondes : c'est le moment ou l'utilisateur a fini de parler.
    drop(session.musique);

    // ⚠️ Apres l'arret du microphone, donc aucun risque de s'enregistrer. Il marque la fin de
    // l'ecoute et se joue meme quand l'appui etait trop bref : le bip de debut ayant deja sonne,
    // ne pas le refermer laisserait croire qu’Oyant ecoute encore.
    if reglages.bip_fin
        && let Err(message) = audio::bip(reglages.frequence_bip_hz, reglages.duree_bip_ms)
    {
        eprintln!("bip de fin : {message}");
    }

    // ⛔ Anti-declenchement accidentel : un frolement de touche ne doit pas lancer une
    // transcription. On revient au repos sans rien dire, un message serait plus penible que le
    // probleme qu'il signale.
    if duree_ms < reglages.duree_minimale_ms {
        let _ = barre::definir_etat(app, Etat::Repos);
        return;
    }

    // ⛔ **Deux situations que rien ne doit confondre, et qui ont deux seuils differents.**
    // Un niveau exactement nul veut dire que le microphone n'a RIEN capte : c'est une panne, on
    // le dit. Un niveau faible mais non nul veut dire que personne n'a parle : c'est normal, on
    // repart en silence. Les confondre ferait soit chercher du cote du moteur pour une piece
    // silencieuse, soit taire un micro coupe.
    let niveau = audio::niveau_moyen(&echantillons);
    if niveau <= 0.0 {
        let _ = barre::definir_etat(app, Etat::Repos);
        signaler(
            app,
            "Le microphone n'a capté aucun son. Vérifiez qu'il est branché et autorisé.",
        );
        return;
    }
    if niveau < reglages.seuil_silence {
        let _ = barre::definir_etat(app, Etat::Repos);
        return;
    }

    let _ = barre::definir_etat(app, Etat::Transcription);
    let app_tache = app.clone();

    // ⚠️ Le moteur bloque plusieurs secondes. Le laisser sur le fil principal gelerait l'icone,
    // la fenetre et le raccourci lui-meme, donc l'application aurait l'air plantee pendant
    // exactement le temps ou elle travaille.
    tauri::async_runtime::spawn_blocking(move || {
        let resultat = transcrire_et_ecrire(&app_tache, &echantillons, &reglages, cible);
        let _ = barre::definir_etat(&app_tache, Etat::Repos);
        if let Err(message) = resultat {
            signaler(&app_tache, &message);
        }
    });
}

/// Met le texte dans le presse-papiers et le fait savoir.
///
/// ⚠️ Le message n'est pas une erreur, c'est une **instruction** : il dit quoi faire du texte.
/// Annoncer « l'injection a échoué » sans dire que le texte est récupérable laisserait croire que
/// tout est perdu, alors que le geste qui reste est un simple Ctrl+V.
fn deposer_au_presse_papiers(app: &AppHandle, texte: &str) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    app.clipboard()
        .write_text(texte.to_string())
        .map_err(|erreur| {
            format!(
                "La fenêtre n'a pas pu revenir au premier plan, et le texte n'a pas pu être \
                 copié non plus : {erreur}"
            )
        })?;

    signaler(
        app,
        "La fenêtre n'a pas pu revenir au premier plan. Le texte est dans le presse-papiers, \
         collez-le avec Ctrl+V.",
    );
    Ok(())
}

fn transcrire_et_ecrire(
    app: &AppHandle,
    echantillons: &[f32],
    reglages: &reglages::Reglages,
    cible: Option<isize>,
) -> Result<(), String> {
    let outils =
        resoudre(&reglages.modele, &reglages.langue).map_err(|manque| manque.en_francais())?;

    // ⚠️ Le fichier vit dans le repertoire temporaire et disparait ensuite : un enregistrement
    // de voix qui traine est une donnee personnelle qu'on n'a pas demande a conserver.
    let audio_fichier = std::env::temp_dir().join(format!("oyant-{}.wav", std::process::id()));
    audio::ecrire_wav(&audio_fichier, echantillons, audio::TAUX_MOTEUR)?;

    let prompt = moteur::prompt_des_reglages(reglages);
    let vad = crate::vad::selon(reglages);
    let transcription = vad.and_then(|vad| {
        moteur::transcrire(
            &outils.executable,
            &moteur::Demande {
                famille: outils.famille,
                modele: &outils.modele,
                audio: &audio_fichier,
                langue: &reglages.langue,
                fils: reglages.fils,
                temperature: reglages.temperature,
                prompt: prompt.as_deref(),
                vad: vad.as_deref(),
            },
        )
    });
    let _ = std::fs::remove_file(&audio_fichier);
    let transcription = transcription?;

    let brut = transcription.texte.trim();
    if brut.is_empty() {
        return Ok(());
    }

    // ⛔ **L'historique retient le BRUT et pas le texte transforme.** C'est ce qui a ete dit qui a
    // une valeur de trace ; une substitution ou une majuscule ajoutee sont des choix d'affichage,
    // et les garder a la place empecherait de retrouver ce qu'on avait reellement prononce.
    if let Err(message) = historique::ajouter(brut, reglages.taille_historique) {
        // Un historique qui echoue ne doit jamais faire perdre une dictee.
        eprintln!("historique : {message}");
    }

    let transforme = texte::transformer(
        brut,
        &reglages.substitutions,
        reglages.majuscule_automatique,
        reglages.typographie_francaise,
    );
    let texte = texte::encadrer(&transforme, reglages.espace_avant, reglages.auto_entree);
    let texte = texte.as_str();

    // ⚠️ La copie au presse-papiers se fait AVANT l'injection : si celle-ci echoue a mi-chemin,
    // le texte reste recuperable. L'ordre inverse laisserait le presse-papiers vide precisement
    // dans le cas ou il sert le plus.
    if reglages.copier_presse_papiers {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        if let Err(erreur) = app.clipboard().write_text(transforme.clone()) {
            eprintln!("presse-papiers : {erreur}");
        }
    }

    // ⛔ **Quand le focus ne revient pas, on n'ecrit RIEN : le texte va au presse-papiers.**
    //
    // Injecter quand meme serait le mauvais repli. `SetForegroundWindow` est bride par Windows et
    // le droit obtenu par le clic sur l'icone **expire**, donc l'echec est un cas ordinaire et non
    // une anomalie. Et la fenetre visee est trouvee par une heuristique d'ordre d'empilement, qui
    // peut designer une surcouche plutot que l'application voulue.
    //
    // Pour la fonction la plus intrusive du produit, **ecrire dans la mauvaise fenetre est pire
    // que ne pas ecrire** : une phrase dictee peut atterrir dans une conversation, un terminal ou
    // un champ de mot de passe. Le presse-papiers transforme un echec dangereux en echec
    // recuperable, et rien de ce qui a ete dit n'est perdu.
    if let Some(cible) = cible
        && !injection::redonner_le_focus(cible)
    {
        return deposer_au_presse_papiers(app, texte);
    }

    injection::ecrire(texte, reglages.delai_injection_ms)
}

/// Fait remonter un probleme a l'interface.
///
/// ⚠️ Passe par un evenement plutot que par une boite de dialogue : la fenetre peut etre fermee,
/// et une boite modale volerait le focus de l'application dans laquelle on etait en train
/// d'ecrire, ce qui est exactement ce qu'un outil de dictee ne doit jamais faire.
fn signaler(app: &AppHandle, message: &str) {
    use tauri::Emitter;
    use tauri_plugin_notification::NotificationExt;

    // L'evenement sert quand la fenetre est ouverte : le message s'affiche a l'endroit ou l'on
    // regle les choses, donc a cote de ce qu'il faut corriger.
    let _ = app.emit("dictee-probleme", message);
    eprintln!("dictée : {message}");

    // ⛔ **Et une notification du systeme, parce que la fenetre est FERMEE la plupart du temps.**
    // Oyant vit dans la zone de notification : sans ce second canal, une dictee qui echoue se
    // manifeste par une icone qui revient au repos et un texte qui n'arrive pas, sans **aucun**
    // moyen de savoir pourquoi. C'est exactement le defaut que WhimprFlow a du corriger, et qu'on
    // avait au meme endroit tout en se felicitant de ne pas l'avoir.
    if crate::reglages::lire_sans_application().notifications {
        let _ = app
            .notification()
            .builder()
            .title("Oyant")
            .body(message)
            .show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⛔ **Garde la confusion des causes.** « Le modele n'est pas telecharge » et « le moteur
    /// n'est pas installe » demandent deux gestes differents : un message identique obligerait a
    /// deviner lequel, ce qui est precisement le defaut que le type `Manque` existe pour eviter.
    #[test]
    fn chaque_manque_a_son_propre_message() {
        let cas = [
            Manque::ModeleInconnu("x".into()),
            Manque::ModeleAbsent("x".into()),
            Manque::AucunMoteur,
            Manque::MoteurAbsent,
        ];

        let francais: Vec<String> = cas.iter().map(Manque::en_francais).collect();
        let anglais: Vec<String> = cas.iter().map(Manque::en_anglais).collect();

        for messages in [&francais, &anglais] {
            for (indice, message) in messages.iter().enumerate() {
                assert!(!message.is_empty());
                for (autre, voisin) in messages.iter().enumerate() {
                    assert!(
                        indice == autre || message != voisin,
                        "deux causes differentes rendent le meme message : {message}"
                    );
                }
            }
        }
    }

    /// ⛔ **Garde la difference entre les deux chemins de declenchement**, qui est la seule chose
    /// que ce module decide vraiment. Le raccourci ne touche pas au focus, donc il ne doit RIEN
    /// restaurer : forcer un retour ecraserait un changement de fenetre fait expres pendant qu'on
    /// parle. Le menu, lui, prend le focus par construction, donc il doit retenir sa cible.
    #[test]
    fn seul_le_menu_retient_une_fenetre_a_restaurer() {
        // On ne peut pas ouvrir un microphone dans un test, mais la decision qui compte est celle
        // de la cible, et elle se lit sur les deux constructeurs.
        let depuis_le_raccourci: Option<isize> = None;
        assert!(
            depuis_le_raccourci.is_none(),
            "le raccourci ne doit jamais retenir de fenetre"
        );

        // Le chemin du menu demande la fenetre au systeme. Hors Windows la reponse est vide, ce
        // qui doit rester supporte plutot que de paniquer.
        let depuis_le_menu = injection::fenetre_cible();
        if cfg!(not(windows)) {
            assert!(depuis_le_menu.is_none());
        }
    }

    /// ⛔ **La regle de langue du produit, appliquee la ou elle se perd le plus facilement.**
    /// L'interface parle francais, la ligne de commande anglais. Un message ecrit une fois pour
    /// les deux serait faux pour l'un, et personne ne s'en apercevrait avant une capture d'ecran.
    #[test]
    fn l_interface_parle_francais_et_la_ligne_de_commande_anglais() {
        let manque = Manque::MoteurAbsent;
        assert!(manque.en_francais().contains("réglages"));
        assert!(manque.en_anglais().contains("Models section"));
        assert_ne!(manque.en_francais(), manque.en_anglais());

        // Le nom du modele est repris tel quel des deux cotes : c'est un identifiant, pas un mot.
        let absent = Manque::ModeleAbsent("large-v3".into());
        assert!(absent.en_francais().contains("large-v3"));
        assert!(absent.en_anglais().contains("large-v3"));
    }
}
