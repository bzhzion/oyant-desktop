//! Un reglage affiche doit piloter quelque chose.
//!
//! ⛔ **Un reglage qu'on montre et qui ne fait rien est un mensonge a l'utilisateur**, et c'est le
//! defaut le plus silencieux du produit : l'ecran a l'air complet, la case se coche, la valeur se
//! sauvegarde, et rien ne change jamais. Il ne se voit qu'en essayant, c'est-a-dire chez celui qui
//! s'en sert.
//!
//! Releve le 2026-09-17 : **seize reglages sur vingt-six** etaient dans ce cas, dont deux
//! appartenant a l'etape qui venait d'etre livree.
//!
//! ⚠️ **La table d'exclusions porte la RAISON de chaque report**, sans quoi ce controle serait
//! rouge en permanence sur des choix assumes, et un controle toujours rouge finit ignore. Un
//! reglage quitte cette table le jour ou son etape le branche : retirer la ligne est alors le
//! geste qui prouve qu'il sert.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Reglages declares mais pas encore branches, avec l'etape qui s'en chargera.
///
/// La cle est le nom du champ Rust, la valeur dit **pourquoi** il attend.
fn reports() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        // Etape 9 et au-dela.
        (
            "transcription_directe",
            "etape 9 : demande de suivre le moteur pendant qu'il travaille",
        ),
        (
            "mise_a_jour_automatique",
            "etape 9 : la mise a jour de l'application n'existe pas",
        ),
        (
            "niveau_journal",
            "etape 9 : le journal n'est pas encore ecrit",
        ),
    ])
}

/// Reglages qui ne concernent QUE l'ecran, et dont le coeur n'a pas a connaitre l'existence.
///
/// ⚠️ Troisieme categorie, distincte des deux autres : ce ne sont ni des reglages muets ni des
/// reports. Les ranger dans les reports serait faux, puisqu'ils fonctionnent deja, et les compter
/// comme branches cote coeur le serait aussi. Leur implementation vit dans le fichier qui declare
/// l'ecran, ce qui les rend invisibles a une recherche « utilise ailleurs ».
fn pilotes_par_l_ecran() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([(
        "afficher_avances",
        "l'interrupteur « réglages avancés » : purement d'affichage, le coeur n'en fait rien",
    )])
}

/// Les champs declares, avec leur nom cote interface.
///
/// ⚠️ **Un reglage porte DEUX noms** : un identifiant Rust en francais et une cle de fichier en
/// anglais, posee par `#[serde(rename = ...)]`. L'interface ne connait que la seconde, donc un
/// controle qui ne chercherait que la premiere accuserait a tort tout reglage pilote uniquement
/// par l'ecran. C'est arrive des le premier essai, sur `afficher_avances`.
fn champs_declares(source: &str) -> Vec<(String, String)> {
    let debut = source
        .find("pub struct Reglages {")
        .expect("la structure Reglages doit exister");
    let corps = &source[debut..];
    let fin = corps.find("\n}").expect("la structure doit se fermer");

    let mut champs = Vec::new();
    let mut cle_interface: Option<String> = None;

    for ligne in corps[..fin].lines() {
        let ligne = ligne.trim();
        if let Some(reste) = ligne.strip_prefix("#[serde(rename = \"")
            && let Some(nom) = reste.split('"').next()
        {
            cle_interface = Some(nom.to_string());
            continue;
        }
        if let Some(reste) = ligne.strip_prefix("pub ")
            && let Some(nom) = reste.split(':').next()
        {
            let nom = nom.trim();
            if !nom.is_empty() && !nom.contains(' ') {
                let interface = cle_interface.take().unwrap_or_else(|| nom.to_string());
                champs.push((nom.to_string(), interface));
            }
        }
    }
    champs
}

/// Un champ pilote-t-il quelque chose, cote coeur ou cote interface ?
fn utilise_ailleurs(champ: &str, cle_interface: &str, sources: &[(String, String)]) -> bool {
    let motif_rust = format!(".{champ}");
    sources.iter().any(|(nom, contenu)| {
        // ⚠️ On exclut les deux fichiers qui DECLARENT les reglages : `reglages.rs` les cite tous
        // pour les normaliser et les serialiser, `reglages.ts` les cite tous pour dessiner
        // l'ecran. Les compter rendrait le controle toujours vert.
        if nom == "reglages.rs" || nom == "reglages.ts" {
            return false;
        }
        if nom.ends_with(".rs") {
            return contenu.contains(&motif_rust);
        }
        // Cote interface le reglage n'existe que sous sa cle anglaise.
        contenu.contains(cle_interface)
    })
}

fn lire_sources(repertoire: &Path) -> Vec<(String, String)> {
    let mut sources = Vec::new();
    let mut a_visiter = vec![repertoire.to_path_buf()];
    while let Some(chemin) = a_visiter.pop() {
        let Ok(entrees) = fs::read_dir(&chemin) else {
            continue;
        };
        for entree in entrees {
            let entree = entree.expect("entree lisible");
            let chemin = entree.path();
            if chemin.is_dir() {
                a_visiter.push(chemin);
            } else if chemin
                .extension()
                .is_some_and(|extension| extension == "rs" || extension == "ts")
            {
                let nom = chemin
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                sources.push((nom, fs::read_to_string(&chemin).unwrap_or_default()));
            }
        }
    }
    sources
}

#[test]
fn chaque_reglage_pilote_quelque_chose_ou_dit_pourquoi_il_attend() {
    let coeur = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // L'interface vit a cote du coeur : un reglage peut n'etre pilote que par elle.
    let interface = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("le depot doit avoir une racine")
        .join("src");

    let mut sources = lire_sources(&coeur);
    sources.extend(lire_sources(&interface));
    let declaration = fs::read_to_string(coeur.join("reglages.rs")).expect("reglages.rs lisible");

    let reports = reports();
    let ecran_seul = pilotes_par_l_ecran();
    let mut muets = Vec::new();
    let mut exclusions_perimees = Vec::new();

    for (champ, cle_interface) in champs_declares(&declaration) {
        let branche = utilise_ailleurs(&champ, &cle_interface, &sources)
            || ecran_seul.contains_key(champ.as_str());
        let reporte = reports.contains_key(champ.as_str());

        if !branche && !reporte {
            muets.push(champ.clone());
        }
        // ⛔ L'autre moitie du controle, et celle qu'on oublie : une exclusion qui ne sert plus
        // doit DISPARAITRE. Sinon la table grossit, decrit un passe et couvre un jour un vrai
        // defaut.
        if branche && reporte {
            exclusions_perimees.push(champ.clone());
        }
    }

    assert!(
        muets.is_empty(),
        "réglages affichés qui ne pilotent rien, et qui ne sont pas déclarés en attente : {muets:?}\n\
         Soit les brancher, soit ajouter leur raison dans `reports()`."
    );
    assert!(
        exclusions_perimees.is_empty(),
        "ces réglages sont branchés : retirer leur ligne de `reports()` : {exclusions_perimees:?}"
    );
}

/// ⛔ **Le modele et le moteur ne se choisissent QU'A UN ENDROIT** : l'ecran de transcription, ou
/// l'on voit leur poids, leur etat et leur telechargement.
///
/// Le modele avait en plus une liste deroulante dans les reglages, et le defaut s'est manifeste
/// exactement comme la regle du menu de l'icone l'annonce depuis le debut : painteau a dicte le
/// 2026-09-18 **sans savoir avec quel modele**, parce que la selection n'etait pas la ou il la
/// cherchait. Un reglage present a deux endroits et un reglage absent de celui ou on le cherche
/// sont le meme defaut vu des deux cotes.
#[test]
fn le_modele_et_le_moteur_ne_sont_pas_proposes_dans_les_reglages() {
    let ecran = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("le depot doit avoir une racine")
        .join("src")
        .join("reglages.ts");
    let contenu = fs::read_to_string(&ecran).expect("reglages.ts de l'interface lisible");

    for interdit in ["cle: 'model'", "cle: 'compute'"] {
        assert!(
            !contenu.contains(interdit),
            "« {interdit} » est déclaré dans l'écran de réglages : il ne doit se choisir que \
             dans l'écran de transcription, avec son téléchargement."
        );
    }
}

/// ⛔ **Un reglage en attente ne doit pas etre AFFICHE.** C'est la moitie qui manquait : le
/// controle verifiait qu'un reglage declare pilote quelque chose, pas qu'un reglage inerte reste
/// invisible. Deux d'entre eux etaient donc montres, et « Mises a jour automatiques » etait
/// **cochee par defaut**, promettant de tenir le logiciel a jour alors que rien n'existait
/// derriere. Une promesse de securite est le pire endroit ou laisser un reglage qui ment.
#[test]
fn un_reglage_en_attente_n_est_pas_montre_a_l_utilisateur() {
    let ecran = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("le depot doit avoir une racine")
        .join("src")
        .join("reglages.ts");
    let contenu = fs::read_to_string(&ecran).expect("reglages.ts de l'interface lisible");

    let declaration = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("reglages.rs"),
    )
    .expect("reglages.rs lisible");
    let cles: BTreeMap<_, _> = champs_declares(&declaration).into_iter().collect();

    let mut montres = Vec::new();
    for champ in reports().keys() {
        let Some(cle_interface) = cles.get(*champ) else {
            continue;
        };
        if contenu.contains(&format!("cle: '{cle_interface}'")) {
            montres.push(format!("{champ} (« {cle_interface} »)"));
        }
    }

    assert!(
        montres.is_empty(),
        "ces réglages ne pilotent rien et sont pourtant affichés : {montres:?}
         Soit les brancher, soit les retirer de l'écran jusqu'à ce qu'ils servent."
    );
}

/// ⚠️ Le controle ne vaut que s'il lit vraiment la structure. Une erreur de decoupage rendrait
/// une liste vide, donc un test vert qui ne verifie rien.
#[test]
fn la_lecture_des_champs_trouve_bien_la_structure() {
    let racine = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let declaration = fs::read_to_string(racine.join("reglages.rs")).expect("reglages.rs lisible");
    let champs = champs_declares(&declaration);

    assert!(
        champs.len() > 20,
        "seulement {} champs trouvés, le découpage est faux",
        champs.len()
    );
    for attendu in ["raccourci", "microphone", "modele", "calcul"] {
        assert!(
            champs.iter().any(|(champ, _)| champ == attendu),
            "champ « {attendu} » non trouvé par la lecture"
        );
    }

    // ⚠️ La correspondance des deux noms se verifie aussi : sans elle le controle chercherait la
    // mauvaise chaine cote interface et accuserait a tort.
    let paires: BTreeMap<_, _> = champs.into_iter().collect();
    assert_eq!(paires.get("raccourci").map(String::as_str), Some("hotkey"));
    assert_eq!(
        paires.get("afficher_avances").map(String::as_str),
        Some("show_advanced")
    );
}
