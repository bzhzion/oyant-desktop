//! Reglages de l'application : le fichier de configuration et les commandes que l'interface
//! appelle.
//!
//! ⛔ **Regle posee par painteau le 2026-09-17 : un reglage vit ICI et nulle part ailleurs.**
//! Le menu de l'icone de la zone de notification ne porte que des ACTIONS (lancer un
//! enregistrement, traduire), jamais un reglage.
//!
//! ⚠️ **Deux sortes de reglages, a ne surtout pas melanger.**
//!
//! - Ceux dont **le systeme est la source de verite** : le demarrage automatique en est un, il
//!   vit dans la cle `Run` de Windows. On ne le recopie **jamais** dans notre fichier. Un miroir
//!   finirait par diverger de l'original, et c'est precisement ce qu'on cherche a eviter.
//! - Ceux dont **nous sommes la source de verite** : tout le reste, dans `reglages.json`.
//!
//! ⚠️ **Les cles du fichier sont en ANGLAIS**, comme la ligne de commande, alors que les
//! identifiants Rust restent en francais comme le reste du depot. Un fichier de configuration est
//! un **format**, pas une interface : il se documente, se partage dans un rapport de bug, et
//! survivra a la traduction de l'interface. Le renommer plus tard couterait une migration.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

// ── Demarrage automatique : etat du SYSTEME, jamais du fichier ─────────────────────────────

/// Etat REEL du demarrage automatique, lu dans le systeme.
///
/// Renvoie `false` si la question ne peut pas etre posee. C'est le repli le moins trompeur des
/// deux : annoncer « inactif » sur une installation reellement active pousse a recocher, ce qui
/// est sans effet de bord ; annoncer « actif » a tort ferait croire le contraire de la realite et
/// ne donnerait aucune occasion de s'en apercevoir.
#[tauri::command]
pub fn demarrage_automatique(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Inscrit ou retire Oyant du demarrage du systeme, puis **relit l'etat reel**.
///
/// La valeur rendue est ce que le systeme dit APRES l'operation, pas ce qui a ete demande. En cas
/// de refus, l'erreur est remontee a l'interface pour y etre affichee : un reglage qui echoue en
/// silence est pire que pas de reglage du tout.
#[tauri::command]
pub fn definir_demarrage_automatique(app: AppHandle, actif: bool) -> Result<bool, String> {
    let lanceur = app.autolaunch();

    let resultat = if actif {
        lanceur.enable()
    } else {
        lanceur.disable()
    };

    if let Err(erreur) = resultat {
        // ⚠️ Message rendu a l'interface, donc en francais : c'est l'utilisateur qui le lit.
        // Les messages de la ligne de commande, eux, sont en anglais (voir le README).
        return Err(format!("Le système a refusé l'opération : {erreur}"));
    }

    Ok(lanceur.is_enabled().unwrap_or(false))
}

// ── Le fichier de configuration ────────────────────────────────────────────────────────────

/// L'ensemble des reglages dont Oyant est la source de verite.
///
/// ⚠️ `#[serde(default)]` au niveau du conteneur est **structurant et pas cosmetique** : un
/// fichier ecrit par une version anterieure n'a pas les champs ajoutes depuis, et sans lui la
/// lecture echouerait entierement. Chaque champ absent reprend sa valeur par defaut, donc ajouter
/// un reglage ne casse jamais la configuration de quelqu'un.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reglages {
    // Capture
    #[serde(rename = "hotkey")]
    pub raccourci: String,
    /// Nom du microphone a utiliser. **Vide veut dire « celui du systeme »**, et c'est le defaut.
    ///
    /// ⚠️ Vide n'est pas « aucun » : c'est le seul reglage qui suive l'utilisateur quand il
    /// branche un casque, alors qu'un nom fige designerait un appareil debranche.
    #[serde(rename = "microphone")]
    pub microphone: String,
    #[serde(rename = "min_duration_ms")]
    pub duree_minimale_ms: u32,
    #[serde(rename = "max_duration_s")]
    pub duree_maximale_s: u32,
    #[serde(rename = "silence_threshold")]
    pub seuil_silence: f32,
    #[serde(rename = "start_beep")]
    pub bip_debut: bool,
    #[serde(rename = "end_beep")]
    pub bip_fin: bool,
    #[serde(rename = "beep_frequency_hz")]
    pub frequence_bip_hz: u32,
    #[serde(rename = "beep_duration_ms")]
    pub duree_bip_ms: u32,
    #[serde(rename = "pause_media")]
    pub pause_medias: bool,

    // Transcription
    #[serde(rename = "model")]
    pub modele: String,
    #[serde(rename = "language")]
    pub langue: String,
    #[serde(rename = "threads")]
    pub fils: u32,
    #[serde(rename = "temperature")]
    pub temperature: f32,
    #[serde(rename = "live_transcription")]
    pub transcription_directe: bool,
    /// Ne donner au moteur que les passages ou quelqu'un parle (Silero VAD). Voir `vad.rs`.
    #[serde(rename = "voice_detection")]
    pub detection_parole: bool,
    /// Sur quoi l'ordinateur fait le calcul : processeur, carte graphique.
    ///
    /// ⚠️ Ce n'est PAS un reglage comme les autres : il designe un programme qu'il faut avoir
    /// telecharge. Il vit donc dans l'ecran de transcription, avec son installation, et pas dans
    /// la liste des reglages ou il serait un choix sans effet tant que rien n'est installe.
    #[serde(rename = "compute")]
    pub calcul: String,

    // Sortie
    #[serde(rename = "injection_delay_ms")]
    pub delai_injection_ms: u32,
    #[serde(rename = "auto_enter")]
    pub auto_entree: bool,
    #[serde(rename = "leading_space")]
    pub espace_avant: bool,
    #[serde(rename = "auto_capitalize")]
    pub majuscule_automatique: bool,
    #[serde(rename = "french_typography")]
    pub typographie_francaise: bool,
    #[serde(rename = "copy_to_clipboard")]
    pub copier_presse_papiers: bool,
    /// Remplacements automatiques definis par l'utilisateur.
    ///
    /// ⚠️ Le contrat de fonctionnalites les listait depuis le depart, et ils manquaient a la
    /// structure : releve en commencant l'etape 8.
    #[serde(rename = "substitutions")]
    pub substitutions: Vec<crate::texte::Substitution>,

    /// Mots que le moteur risque de mal entendre : noms propres, termes de metier, acronymes.
    ///
    /// ⛔ **Ce n'est pas une liste de substitutions, et les confondre serait l'erreur.** Une
    /// substitution corrige le texte APRES coup, de façon exacte et sans discernement. Ce
    /// vocabulaire est donne au moteur AVANT la transcription, pour qu'il se trompe moins : c'est
    /// un biais souple, donc rien ne peut etre remplace a tort. Les projets qui font ça le mieux
    /// separent nettement les deux, et c'est ce qu'on reprend.
    ///
    /// ⚠️ Il n'y a pas de correction approximative apres coup pour l'instant, et c'est
    /// deliberement remis a plus tard : on mesure d'abord ce que ce biais suffit a corriger. Sur
    /// un public tenu au secret, une correction fausse coute plus qu'une correction manquee.
    #[serde(rename = "vocabulary", default)]
    pub vocabulaire: Vec<String>,

    // Interface
    #[serde(rename = "notifications")]
    pub notifications: bool,
    #[serde(rename = "history_size")]
    pub taille_historique: u32,

    // Maintenance
    #[serde(rename = "auto_update")]
    pub mise_a_jour_automatique: bool,
    #[serde(rename = "log_level")]
    pub niveau_journal: String,

    /// Etat de l'ecran et non du produit : les reglages avances sont-ils deplies ?
    ///
    /// Persiste volontairement. Quelqu'un qui se sert des reglages avances les veut visibles a
    /// chaque ouverture, et les lui replier a chaque fois serait une brimade.
    #[serde(rename = "show_advanced")]
    pub afficher_avances: bool,

    /// Accepter qu'un telephone se connecte a cet ordinateur.
    ///
    /// ⛔ **Faux par defaut, et ce defaut est la moitie de la securite de cette fonction.** Le
    /// serveur fait injecter du texte dans les applications de l'utilisateur : il ne s'ouvre que
    /// parce que quelqu'un l'a demande, jamais parce qu'on l'a installe.
    #[serde(rename = "network_enabled", default)]
    pub reseau_actif: bool,

    /// Ecouter sur toutes les interfaces au lieu de la seule boucle locale.
    ///
    /// ⛔ **Faux par defaut.** Regle deja appliquee a BeamMeUp et Hublot dans le parc : un service
    /// local ne s'expose au reseau que sur un geste delibere. ⚠️ Tant qu'il est faux, aucun
    /// telephone ne peut joindre l'ordinateur — c'est voulu, le defaut protege celui qui n'a rien
    /// demande.
    #[serde(rename = "network_all_interfaces", default)]
    pub reseau_toutes_interfaces: bool,
}

impl Default for Reglages {
    fn default() -> Self {
        Self {
            raccourci: "Ctrl+Alt+Space".to_string(),
            microphone: String::new(),
            duree_minimale_ms: 300,
            duree_maximale_s: 120,
            seuil_silence: 0.01,
            bip_debut: true,
            bip_fin: true,
            frequence_bip_hz: 880,
            duree_bip_ms: 80,
            pause_medias: false,

            // ⛔ **Le plus LEGER du catalogue, et c'est lie au moteur par defaut.** Une
            // installation neuve calcule sur le processeur : `medium` y met 22 s pour dix
            // secondes d'audio, mesure le 2026-09-20 sur l'installation reelle, et demande
            // 1,5 Go avant la premiere dictee. `small` fait 487 Mo et environ trois fois plus
            // vite, contre un peu de precision. Le defaut sert la premiere minute de quelqu'un
            // qui decouvre, pas le meilleur resultat possible.
            modele: "small".to_string(),
            langue: "auto".to_string(),
            // 4 fils : un defaut qui tient sur une machine modeste. Le mesurer viendra avec le
            // moteur, a l'etape 6 ; l'inventer ici serait un chiffre declare et pas derive.
            fils: 4,
            temperature: 0.0,
            transcription_directe: false,
            // ⛔ Active par defaut, y compris pour une configuration existante (`serde(default)`) :
            // sans lui, un souffle de ventilation devient une phrase tapee dans l'application de
            // l'utilisateur. Le couper ne sert qu'a quelqu'un qui chuchote.
            detection_parole: true,
            calcul: "windows-x64-cpu".to_string(),

            // ⛔ ZERO. Il valait 50 ms, soit une pause APRES CHAQUE CARACTERE : une phrase de
            // cent caracteres mettait cinq secondes a s'ecrire. Ne le remonter que pour une
            // application qui perd des caracteres arrivant trop vite.
            delai_injection_ms: 0,
            auto_entree: false,
            espace_avant: false,
            majuscule_automatique: true,
            typographie_francaise: true,
            copier_presse_papiers: false,
            substitutions: Vec::new(),
            vocabulaire: Vec::new(),

            notifications: true,
            // ⛔ **Zero, donc desactive.** L'historique contient ce que quelqu'un a dit a voix
            // haute chez lui ; il s'active a la demande et jamais par defaut. Il valait 20
            // jusqu'au 2026-09-18.
            taille_historique: 0,

            mise_a_jour_automatique: true,
            niveau_journal: "info".to_string(),

            afficher_avances: false,

            reseau_actif: false,
            reseau_toutes_interfaces: false,
        }
    }
}

/// Valeurs acceptees pour les reglages a choix ferme.
///
/// ⚠️ Un fichier de configuration est modifiable a la main : il faut donc traiter son contenu
/// comme une entree non fiable, exactement comme une saisie. Une valeur hors liste est ramenee au
/// defaut plutot que propagee.
///
/// ⛔ **La liste des modeles n'est PAS recopiee ici, elle vient du catalogue.** Elle l'a ete, et
/// les deux avaient deja diverge : la copie portait `parakeet` et ignorait `small`, donc choisir
/// Small le faisait ramener a Medium **en silence** a la premiere relecture du fichier. Defaut
/// trouve le 2026-09-17 avant d'avoir pu nuire, et exactement celui contre lequel l'ecran des
/// modeles se premunit deja en derivant sa propre liste.
const NIVEAUX_JOURNAL: [&str; 4] = ["error", "warn", "info", "debug"];

fn modele_connu(identifiant: &str) -> bool {
    crate::modeles::par_identifiant(identifiant).is_some()
}

impl Reglages {
    /// Ramene chaque valeur dans son domaine.
    ///
    /// ⚠️ Applique **a la lecture ET a l'ecriture**. A la lecture parce que le fichier a pu etre
    /// edite a la main ; a l'ecriture parce que l'interface n'est pas une garantie (les bornes
    /// `min`/`max` d'un champ HTML ne sont pas appliquees a une valeur posee par programme).
    pub fn normaliser(&mut self) {
        let defauts = Reglages::default();

        self.duree_minimale_ms = self.duree_minimale_ms.clamp(0, 5_000);
        self.duree_maximale_s = self.duree_maximale_s.clamp(5, 3_600);
        self.seuil_silence = self.seuil_silence.clamp(0.0, 1.0);
        self.frequence_bip_hz = self.frequence_bip_hz.clamp(100, 8_000);
        self.duree_bip_ms = self.duree_bip_ms.clamp(10, 1_000);

        self.fils = self.fils.clamp(1, 64);
        self.temperature = self.temperature.clamp(0.0, 1.0);

        self.delai_injection_ms = self.delai_injection_ms.clamp(0, 2_000);
        // ⛔ **La borne basse est ZERO et pas un.** Elle valait `1` jusqu'au 2026-09-18, ce qui
        // rendait l'historique impossible a desactiver : le reglage acceptait `0` a l'ecriture et
        // le ramenait a `1` en silence, donc une transcription restait gardee quoi qu'on demande.
        self.taille_historique = self.taille_historique.min(100);

        if !modele_connu(&self.modele) {
            self.modele = defauts.modele;
        }
        if !NIVEAUX_JOURNAL.contains(&self.niveau_journal.as_str()) {
            self.niveau_journal = defauts.niveau_journal;
        }
        if self.langue.trim().is_empty() {
            self.langue = defauts.langue;
        }
        if self.raccourci.trim().is_empty() {
            self.raccourci = defauts.raccourci;
        }
        // Meme regle que pour le modele : une valeur hors catalogue revient au defaut plutot que
        // d'etre propagee. Le fichier s'edite a la main.
        if crate::moteur::par_identifiant(&self.calcul).is_none() {
            self.calcul = defauts.calcul;
        }

        // ⚠️ Une duree minimale superieure a la maximale rendrait toute dictee impossible sans
        // qu'aucun message ne l'explique. On les remet dans l'ordre plutot que de refuser.
        if u64::from(self.duree_minimale_ms) > u64::from(self.duree_maximale_s) * 1_000 {
            self.duree_minimale_ms = defauts.duree_minimale_ms;
        }
    }
}

/// Emplacement du fichier de configuration.
///
/// ⛔ **Surtout PAS dans le repertoire d'installation.** C'est ce que faisait l'ancienne version
/// (`%LOCALAPPDATA%\Oyant\config.json`), avec deux consequences : la desinstallation emporte la
/// configuration, et le repertoire est entre en collision avec l'installation de la nouvelle
/// version le 2026-09-17. Le repertoire de configuration de l'utilisateur est fait pour ca.
fn chemin(_app: &AppHandle) -> Result<PathBuf, String> {
    // ⚠️ Passe par `chemins`, PAS par `app.path()` : la ligne de commande doit lire EXACTEMENT
    // le meme fichier que l'interface, et elle n'a pas d'application Tauri sous la main.
    crate::chemins::reglages()
}

/// Lit les reglages. **Ne refuse jamais de demarrer.**
///
/// ⚠️ Un fichier illisible ou corrompu rend les valeurs par defaut au lieu de faire echouer
/// l'appel : un ecran de reglages vide serait une impasse, alors que des valeurs par defaut
/// restent utilisables et se re-enregistrent au premier changement.
///
/// ⚠️ Le fichier fautif est **conserve** sous `.invalide` plutot qu'ecrase : il porte les
/// reglages de quelqu'un, et il est la seule piece a conviction si le defaut vient de nous.
#[tauri::command]
pub fn lire_reglages(app: AppHandle) -> Reglages {
    let mut reglages = match chemin(&app) {
        Ok(fichier) => match fs::read_to_string(&fichier) {
            Ok(contenu) => match serde_json::from_str::<Reglages>(&contenu) {
                Ok(lus) => lus,
                Err(erreur) => {
                    eprintln!("Settings file is invalid ({erreur}), falling back to defaults");
                    let _ = fs::rename(&fichier, fichier.with_extension("json.invalide"));
                    Reglages::default()
                }
            },
            // Absent au premier lancement : ce n'est pas une anomalie.
            Err(_) => Reglages::default(),
        },
        Err(erreur) => {
            eprintln!("{erreur}");
            Reglages::default()
        }
    };

    reglages.normaliser();
    reglages
}

/// Ecrit les reglages, **normalises**, et rend ce qui a reellement ete ecrit.
///
/// ⚠️ La valeur rendue est celle du disque, pas celle recue : si une borne a ete appliquee,
/// l'interface doit le refleter immediatement plutot que d'afficher une valeur que le fichier ne
/// porte pas.
///
/// ⚠️ **Ecriture en deux temps** (fichier temporaire puis renommage) : une coupure au milieu d'un
/// `write` laisserait un JSON tronque, donc une configuration perdue au prochain demarrage. Le
/// renommage, lui, est atomique.
#[tauri::command]
pub fn ecrire_reglages(app: AppHandle, reglages: Reglages) -> Result<Reglages, String> {
    let mut reglages = reglages;
    reglages.normaliser();

    let fichier = chemin(&app)?;
    if let Some(dossier) = fichier.parent() {
        fs::create_dir_all(dossier)
            .map_err(|erreur| format!("Répertoire de configuration non créé : {erreur}"))?;
    }

    let contenu = serde_json::to_string_pretty(&reglages)
        .map_err(|erreur| format!("Réglages non sérialisables : {erreur}"))?;

    let temporaire = fichier.with_extension("json.tmp");
    fs::write(&temporaire, contenu.as_bytes())
        .map_err(|erreur| format!("Écriture impossible : {erreur}"))?;
    fs::rename(&temporaire, &fichier)
        .map_err(|erreur| format!("Enregistrement impossible : {erreur}"))?;

    Ok(reglages)
}

/// Lit les reglages SANS application Tauri, pour la ligne de commande.
///
/// ⛔ **La ligne de commande doit se comporter exactement comme l'interface.** Si elle ignorait le
/// fichier de reglages, `oyant fichier.wav` utiliserait un autre modele et une autre langue que
/// ce que l'ecran affiche, et tout diagnostic deviendrait impossible : deux resultats differents
/// sur la meme machine sans que rien n'explique pourquoi.
///
/// Ne refuse jamais : un fichier illisible rend les valeurs par defaut, comme cote interface.
pub fn lire_sans_application() -> Reglages {
    let mut reglages = crate::chemins::reglages()
        .ok()
        .and_then(|fichier| fs::read_to_string(fichier).ok())
        .and_then(|contenu| serde_json::from_str::<Reglages>(&contenu).ok())
        .unwrap_or_default();
    reglages.normaliser();
    reglages
}

/// Les valeurs par defaut, telles que le coeur les connait.
///
/// ⚠️ Existe pour que l'interface puisse dire « ce reglage avance n'est plus a sa valeur par
/// defaut » **sans recopier les defauts de son cote**. Deux listes de defauts finiraient par
/// diverger, et l'ecran annoncerait alors des ecarts qui n'existent pas, ou pire, en tairait.
#[tauri::command]
pub fn reglages_par_defaut() -> Reglages {
    Reglages::default()
}

/// Chemin du fichier, pour que l'interface puisse le montrer.
///
/// Savoir OU vivent ses reglages est ce qui permet a quelqu'un de les sauvegarder, de les
/// comparer, ou de nous les envoyer quand quelque chose ne va pas.
#[tauri::command]
pub fn chemin_reglages(app: AppHandle) -> String {
    chemin(&app)
        .map(|c| c.display().to_string())
        .unwrap_or_else(|erreur| erreur)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⛔ **Le modele par defaut doit etre le plus LEGER du catalogue**, parce que le moteur par
    /// defaut est le processeur embarque. Ecrire `assert_eq!(defaut, "small")` serait une
    /// tautologie : on verifie la RAISON, donc qu'aucun modele du catalogue ne pese moins que
    /// celui qu'on propose d'emblee. Le defaut sert la premiere minute de quelqu'un qui decouvre,
    /// et `medium` lui demandait 1,5 Go de telechargement puis 22 s par dictee.
    #[test]
    fn le_modele_par_defaut_est_le_plus_leger_du_catalogue() {
        let defaut = Reglages::default().modele;
        let propose = crate::modeles::par_identifiant(&defaut)
            .expect("le modele par defaut doit exister au catalogue");

        for modele in crate::modeles::CATALOGUE {
            assert!(
                modele.taille >= propose.taille,
                "« {} » pese {} octets et le defaut « {defaut} » en pese {} : le defaut n'est pas le plus leger",
                modele.identifiant,
                modele.taille,
                propose.taille
            );
        }
    }

    /// ⚠️ Le contrat de `#[serde(default)]` : un fichier ecrit par une version anterieure, donc
    /// amputé des champs ajoutes depuis, doit se lire sans erreur. Sans ce comportement, chaque
    /// nouveau reglage effacerait la configuration de tout le monde.
    #[test]
    fn un_fichier_partiel_se_lit_et_complete_avec_les_defauts() {
        let lus: Reglages = serde_json::from_str(r#"{"model":"large-v3"}"#).unwrap();
        assert_eq!(lus.modele, "large-v3");
        assert_eq!(lus.taille_historique, Reglages::default().taille_historique);
    }

    /// Un fichier qui porte une cle inconnue (version PLUS RECENTE, ou reliquat) ne doit pas
    /// faire echouer la lecture non plus.
    #[test]
    fn une_cle_inconnue_est_ignoree_sans_echec() {
        let lus: Reglages =
            serde_json::from_str(r#"{"model":"medium","reglage_du_futur":42}"#).unwrap();
        assert_eq!(lus.modele, "medium");
    }

    /// ⚠️ Les cles du fichier sont en anglais. Ce test echoue si un identifiant francais fuit
    /// dans le format, ce qui couterait une migration une fois des fichiers dans la nature.
    #[test]
    fn les_cles_du_fichier_sont_en_anglais() {
        let json = serde_json::to_string(&Reglages::default()).unwrap();
        let valeur: serde_json::Value = serde_json::from_str(&json).unwrap();
        let objet = valeur.as_object().unwrap();

        for cle in objet.keys() {
            assert!(
                cle.is_ascii(),
                "{cle} : une cle ne doit porter aucun caractere accentue"
            );
        }
        // Quelques cles attendues, pour attraper un renommage accidentel.
        for attendue in ["hotkey", "model", "language", "auto_enter", "history_size"] {
            assert!(objet.contains_key(attendue), "cle manquante : {attendue}");
        }
        // Et aucun des noms francais correspondants.
        for francaise in ["raccourci", "modele", "langue", "auto_entree"] {
            assert!(
                !objet.contains_key(francaise),
                "cle francaise dans le format : {francaise}"
            );
        }
    }

    #[test]
    fn un_aller_retour_ne_perd_rien() {
        let avant = Reglages::default();
        let json = serde_json::to_string(&avant).unwrap();
        let apres: Reglages = serde_json::from_str(&json).unwrap();
        assert_eq!(avant, apres);
    }

    /// ⛔ **L'historique est desactive par defaut, et ce test existe pour que ca le reste.**
    ///
    /// Il contient le texte de ce que quelqu'un a dit a voix haute chez lui, souvent dans
    /// l'exercice d'une profession tenue au secret : il s'active a la demande et jamais par
    /// defaut. Il valait 20 jusqu'au 2026-09-18.
    ///
    /// ⚠️ Sans ce test, remettre une valeur « serviable » ne casse RIEN : l'application marche
    /// mieux du point de vue de celui qui fait le changement, et le defaut de confidentialite ne
    /// se voit nulle part. C'est exactement le genre de regression qu'aucune relecture n'attrape,
    /// parce qu'elle ressemble a une amelioration.
    #[test]
    fn l_historique_est_desactive_par_defaut() {
        assert_eq!(
            Reglages::default().taille_historique,
            0,
            "l'historique doit valoir zero par defaut : il contient ce que l'utilisateur a dit"
        );

        // Et la normalisation ne doit pas le relever en douce, comme elle le faisait quand la
        // borne basse valait 1 : le reglage acceptait 0 a l'ecriture et le coeur le ramenait a 1.
        let mut par_defaut = Reglages::default();
        par_defaut.normaliser();
        assert_eq!(
            par_defaut.taille_historique, 0,
            "normaliser() ne doit pas remonter un historique desactive"
        );
    }

    /// ⚠️ Le fichier est modifiable a la main : son contenu est une entree non fiable.
    #[test]
    fn les_valeurs_hors_domaine_sont_ramenees_dans_leurs_bornes() {
        let mut r = Reglages {
            taille_historique: 5_000,
            fils: 0,
            temperature: 9.5,
            delai_injection_ms: 999_999,
            ..Default::default()
        };
        r.normaliser();

        assert_eq!(r.taille_historique, 100);
        assert_eq!(r.fils, 1);
        assert_eq!(r.temperature, 1.0);
        assert_eq!(r.delai_injection_ms, 2_000);
    }

    /// ⛔ Garde la regression du 2026-09-17 : une liste de modeles recopiee dans ce fichier avait
    /// diverge du catalogue, et `small` y manquait. Le choisir le faisait ramener a Medium en
    /// silence, a la premiere relecture du fichier.
    #[test]
    fn tous_les_modeles_du_catalogue_sont_acceptes() {
        for modele in crate::modeles::CATALOGUE {
            let mut r = Reglages {
                modele: modele.identifiant.to_string(),
                ..Default::default()
            };
            r.normaliser();
            assert_eq!(
                r.modele, modele.identifiant,
                "{} est au catalogue mais refuse par les reglages",
                modele.identifiant
            );
        }
    }

    #[test]
    fn un_modele_inconnu_revient_au_defaut() {
        let mut r = Reglages {
            modele: "gpu-layers".to_string(),
            niveau_journal: "verbeux".to_string(),
            ..Default::default()
        };
        r.normaliser();

        assert_eq!(r.modele, Reglages::default().modele);
        assert_eq!(r.niveau_journal, Reglages::default().niveau_journal);
    }

    /// ⚠️ Une duree minimale plus longue que la maximale rendrait toute dictee impossible, et
    /// **rien ne l'expliquerait a l'ecran**. On remet la valeur par defaut plutot que de laisser
    /// une configuration qui ne peut rien produire.
    #[test]
    fn une_duree_minimale_absurde_ne_bloque_pas_la_dictee() {
        let mut r = Reglages {
            duree_minimale_ms: 600_000,
            duree_maximale_s: 10,
            ..Default::default()
        };
        r.normaliser();

        assert!(u64::from(r.duree_minimale_ms) <= u64::from(r.duree_maximale_s) * 1_000);
    }

    /// Les defauts doivent eux-memes etre dans leurs bornes : sans ce controle, une valeur par
    /// defaut fautive serait corrigee en silence a chaque lecture et personne ne le verrait.
    #[test]
    fn les_defauts_sont_deja_normalises() {
        let defauts = Reglages::default();
        let mut normalises = defauts.clone();
        normalises.normaliser();
        assert_eq!(defauts, normalises);
    }
}
