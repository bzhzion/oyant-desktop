//! Capture du microphone, et mise au format qu'attend le moteur.
//!
//! ⛔ **Whisper n'accepte QUE du 16 kHz mono.** Ce n'est pas une preference : le modele est
//! entraine sur cette frequence, et lui donner autre chose ne produit pas une transcription
//! moins bonne, ca produit du charabia ou une erreur. Or presque aucun microphone ne capture a
//! 16 kHz : les cartes son modernes tournent a 44,1 ou 48 kHz. La conversion est donc obligatoire
//! et c'est la seule partie de ce module qui merite de l'attention.
//!
//! ⚠️ **On demande quand meme 16 kHz a la carte quand elle sait le faire**, parce qu'une
//! conversion evitee est une conversion qui ne peut pas degrader le signal.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// La seule frequence que le moteur accepte.
pub const TAUX_MOTEUR: u32 = 16_000;

/// Un enregistrement en cours. L'arreter rend les echantillons prets pour le moteur.
pub struct Enregistrement {
    flux: cpal::Stream,
    echantillons: Arc<Mutex<Vec<f32>>>,
    taux_source: u32,
    canaux: u16,
}

/// Les microphones que la machine expose, par leur nom d'affichage.
///
/// ⚠️ Le peripherique par defaut est rendu en premier et marque, parce que « celui du systeme »
/// est le choix que veut la quasi-totalite des gens et qu'il ne porte pas toujours un nom
/// reconnaissable.
pub fn microphones() -> Vec<String> {
    let hote = cpal::default_host();
    let defaut = hote.default_input_device().and_then(nom_de);

    let mut noms = Vec::new();
    if let Some(nom) = defaut.clone() {
        noms.push(nom);
    }
    if let Ok(peripheriques) = hote.input_devices() {
        for peripherique in peripheriques {
            if let Some(nom) = nom_de(peripherique)
                && Some(&nom) != defaut.as_ref()
            {
                noms.push(nom);
            }
        }
    }
    noms
}

/// Le nom affichable d'un peripherique.
///
/// ⚠️ En cpal 0.18 il ne vient plus d'un `name()` direct mais d'une `description()`, qui peut
/// echouer : un peripherique debranche entre l'enumeration et la lecture n'a plus de description.
fn nom_de(peripherique: cpal::Device) -> Option<String> {
    peripherique
        .description()
        .ok()
        .map(|description| description.name().to_string())
}

fn peripherique(souhaite: &str) -> Result<cpal::Device, String> {
    let hote = cpal::default_host();
    // ⚠️ Un nom vide veut dire « celui du systeme », et pas « aucun ». C'est le reglage par
    // defaut, et le seul qui suive l'utilisateur quand il change de casque.
    if souhaite.is_empty() {
        return hote
            .default_input_device()
            .ok_or_else(|| "Aucun microphone n'est disponible.".to_string());
    }
    if let Ok(mut peripheriques) = hote.input_devices()
        && let Some(trouve) = peripheriques.find(|p| {
            p.description()
                .map(|description| description.name() == souhaite)
                .unwrap_or(false)
        })
    {
        return Ok(trouve);
    }
    // ⛔ On ne retombe PAS en silence sur le peripherique par defaut : quelqu'un qui a choisi un
    // micro precis et qui l'a debranche doit l'apprendre, pas dicter sans comprendre pourquoi le
    // son est mauvais.
    Err(format!(
        "Le microphone « {souhaite} » n'est pas disponible."
    ))
}

/// Ajoute des echantillons sans jamais depasser le plafond.
///
/// ⚠️ Un plafond nul veut dire « pas de limite » : c'est ce que produit un reglage a zero, et
/// couper l'enregistrement a zero echantillon serait une lecture absurde de ce reglage.
///
/// Fonction PURE, appelee depuis le fil audio ou l'on ne peut rien afficher.
fn ajouter(tampon: &mut Vec<f32>, valeurs: impl Iterator<Item = f32>, plafond: usize) {
    if plafond == 0 {
        tampon.extend(valeurs);
        return;
    }
    let reste = plafond.saturating_sub(tampon.len());
    tampon.extend(valeurs.take(reste));
}

/// Combien d'echantillons representent la duree maximale, pour cette carte son.
///
/// ⛔ **Sans plafond, une touche coincee enregistre jusqu'a saturer la memoire**, puis envoie des
/// heures d'audio au moteur. Le plafond se calcule en echantillons parce que c'est la seule unite
/// que le fil audio manipule : y comparer une horloge obligerait a lire le temps a chaque salve.
///
/// Fonction PURE, testable sans carte son.
pub fn plafond_echantillons(taux: u32, canaux: u16, secondes: u32) -> usize {
    taux as usize * canaux.max(1) as usize * secondes as usize
}

/// Ouvre le microphone et commence a accumuler le son.
///
/// `duree_maximale_s` borne l'enregistrement : au-dela, le son est ignore plutot qu'accumule.
pub fn demarrer(microphone: &str, duree_maximale_s: u32) -> Result<Enregistrement, String> {
    let peripherique = peripherique(microphone)?;

    let configuration = configuration_preferee(&peripherique)?;
    // ⚠️ En cpal 0.18 `SampleRate` est un alias de `u32` et non plus un tuple : pas de `.0`.
    let taux_source = configuration.sample_rate();
    let canaux = configuration.channels();
    let format = configuration.sample_format();
    let configuration: cpal::StreamConfig = configuration.into();

    let echantillons: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let plafond = plafond_echantillons(taux_source, canaux, duree_maximale_s);

    // ⚠️ Une erreur de flux survient sur le fil audio, ou l'on ne peut rien afficher. On la
    // journalise et l'enregistrement se terminera simplement plus court : mieux vaut un extrait
    // que rien, et l'utilisateur verra le texte manquant.
    let sur_erreur = |erreur| eprintln!("flux audio : {erreur}");

    let flux = match format {
        cpal::SampleFormat::F32 => peripherique.build_input_stream(
            configuration,
            {
                let accumulateur = echantillons.clone();
                move |donnees: &[f32], _| {
                    if let Ok(mut tampon) = accumulateur.lock() {
                        ajouter(&mut tampon, donnees.iter().copied(), plafond);
                    }
                }
            },
            sur_erreur,
            None,
        ),
        cpal::SampleFormat::I16 => peripherique.build_input_stream(
            configuration,
            {
                let accumulateur = echantillons.clone();
                move |donnees: &[i16], _| {
                    if let Ok(mut tampon) = accumulateur.lock() {
                        let convertis = donnees.iter().map(|&valeur| valeur as f32 / 32768.0);
                        ajouter(&mut tampon, convertis, plafond);
                    }
                }
            },
            sur_erreur,
            None,
        ),
        cpal::SampleFormat::U16 => peripherique.build_input_stream(
            configuration,
            {
                let accumulateur = echantillons.clone();
                move |donnees: &[u16], _| {
                    if let Ok(mut tampon) = accumulateur.lock() {
                        let convertis = donnees
                            .iter()
                            .map(|&valeur| (valeur as f32 - 32768.0) / 32768.0);
                        ajouter(&mut tampon, convertis, plafond);
                    }
                }
            },
            sur_erreur,
            None,
        ),
        cpal::SampleFormat::U8 => peripherique.build_input_stream(
            configuration,
            {
                let accumulateur = echantillons.clone();
                move |donnees: &[u8], _| {
                    if let Ok(mut tampon) = accumulateur.lock() {
                        // ⚠️ Origine a 128, pas a 0 : voir la doc de `cpal::SampleFormat::U8`.
                        let convertis = donnees
                            .iter()
                            .map(|&valeur| (valeur as f32 - 128.0) / 128.0);
                        ajouter(&mut tampon, convertis, plafond);
                    }
                }
            },
            sur_erreur,
            None,
        ),
        autre => return Err(format!("Format audio non pris en charge : {autre:?}")),
    }
    .map_err(|erreur| format!("Microphone inutilisable : {erreur}"))?;

    flux.play()
        .map_err(|erreur| format!("Microphone impossible à démarrer : {erreur}"))?;

    Ok(Enregistrement {
        flux,
        echantillons,
        taux_source,
        canaux,
    })
}

/// Les formats d'echantillon que ce module sait convertir vers le moteur.
///
/// ⛔ **Doit rester synchronise avec les branches du `match` dans `demarrer`** : y accepter un
/// format sans lui donner sa branche de conversion referait choisir une configuration qu'on
/// refuse ensuite de streamer — exactement le bug que `choisir_configuration` corrige.
fn format_gere(format: cpal::SampleFormat) -> bool {
    matches!(
        format,
        cpal::SampleFormat::F32
            | cpal::SampleFormat::I16
            | cpal::SampleFormat::U16
            | cpal::SampleFormat::U8
    )
}

/// Choisit, parmi les plages proposees, celle qui evite une conversion — en ne considerant QUE
/// les formats qu'on sait effectivement convertir.
///
/// ⛔ **Corrige un vrai echec, trouve en dictant pour de vrai et pas en relecture** : la version
/// precedente prenait la PREMIERE plage mono a 16 kHz venue, sans regarder son format. Un
/// microphone qui n'expose ce mono 16 kHz qu'en U8 (8 bits) faisait choisir cette configuration,
/// que `demarrer` refusait ensuite de streamer — « Format audio non pris en charge : U8 », a
/// CHAQUE appui du raccourci, sans qu'aucun test ne l'ait vu puisqu'aucun test ne tourne contre
/// un vrai peripherique. Filtrer le format AVANT de retenir la plage « ideale » evite de refaire
/// la meme erreur sur un futur format qu'on ne gere pas (I8, I24, I32...).
///
/// Fonction PURE, testable sans carte son : `cpal::SupportedStreamConfigRange` se construit sans
/// rien ouvrir (`::new`), ce qui permet de rejouer ce bug sans microphone U8 sous la main.
fn choisir_configuration(
    plages: impl Iterator<Item = cpal::SupportedStreamConfigRange>,
) -> Option<cpal::SupportedStreamConfig> {
    let mut defaut_possible = None;
    for plage in plages {
        if !format_gere(plage.sample_format()) {
            continue;
        }
        // Mono a 16 kHz exactement : le cas ideal, aucune conversion ensuite.
        if plage.channels() == 1
            && plage.min_sample_rate() <= TAUX_MOTEUR
            && plage.max_sample_rate() >= TAUX_MOTEUR
        {
            return Some(plage.with_sample_rate(TAUX_MOTEUR));
        }
        defaut_possible.get_or_insert(plage);
    }
    // A defaut d'un mono 16 kHz, au moins une plage dans un format GERE : mieux que l'echec sur
    // la toute premiere plage venue, qui pourrait etre dans un format qu'on ne sait pas streamer.
    defaut_possible.map(cpal::SupportedStreamConfigRange::with_max_sample_rate)
}

/// Choisit une configuration d'entree, en privilegiant celle qui evite une conversion.
fn configuration_preferee(
    peripherique: &cpal::Device,
) -> Result<cpal::SupportedStreamConfig, String> {
    let disponibles = peripherique
        .supported_input_configs()
        .map_err(|erreur| format!("Microphone illisible : {erreur}"))?;

    if let Some(configuration) = choisir_configuration(disponibles) {
        return Ok(configuration);
    }

    // ⚠️ Aucune plage dans un format gere : repli sur le defaut de la carte. `demarrer` peut
    // encore le refuser si meme ce defaut n'est dans aucun format gere — cas rarissime qu'on
    // n'invente pas de solution pour tant qu'il ne s'est jamais produit, mais le message reste
    // honnete sur ce qui a ete essaye plutot que de masquer l'echec.
    peripherique
        .default_input_config()
        .map_err(|erreur| format!("Microphone sans configuration utilisable : {erreur}"))
}

impl Enregistrement {
    /// Combien de temps s'est ecoule, en millisecondes, d'apres le son reellement capture.
    ///
    /// ⚠️ Mesure sur les echantillons et pas sur l'horloge : c'est la duree du son qu'on a, la
    /// seule qui compte pour decider si l'appui etait trop bref. Une horloge compterait aussi le
    /// temps ou le microphone n'a rien envoye.
    pub fn duree_ms(&self) -> u32 {
        let total = self
            .echantillons
            .lock()
            .map(|tampon| tampon.len())
            .unwrap_or(0);
        let par_canal = total / self.canaux.max(1) as usize;
        (par_canal as u64 * 1000 / self.taux_source.max(1) as u64) as u32
    }

    /// Arrete la capture et rend le son au format du moteur : 16 kHz, mono.
    pub fn arreter(self) -> Vec<f32> {
        drop(self.flux);
        let brut = self
            .echantillons
            .lock()
            .map(|tampon| tampon.clone())
            .unwrap_or_default();
        let mono = vers_mono(&brut, self.canaux);
        reechantillonner(&mono, self.taux_source, TAUX_MOTEUR)
    }
}

/// Replie les canaux en un seul, en les moyennant.
///
/// ⚠️ Moyenner et non prendre le premier canal : sur beaucoup de casques un des deux canaux est
/// muet, et garder celui-la donnerait un silence parfait sans le moindre message d'erreur.
pub fn vers_mono(entrelace: &[f32], canaux: u16) -> Vec<f32> {
    let canaux = canaux.max(1) as usize;
    if canaux == 1 {
        return entrelace.to_vec();
    }
    entrelace
        .chunks_exact(canaux)
        .map(|trame| trame.iter().sum::<f32>() / canaux as f32)
        .collect()
}

/// Convertit vers une autre frequence, en moyennant les echantillons de chaque fenetre.
///
/// ⛔ **Moyenner n'est pas une commodite, c'est ce qui evite le repliement.** Se contenter de
/// prendre un echantillon sur trois pour passer de 48 a 16 kHz replie tout ce qui depasse 8 kHz
/// dans la bande utile, et les consonnes fricatives (`s`, `f`, `ch`) y ont justement de
/// l'energie : le moteur recoit alors un sifflement a la place. La moyenne sur la fenetre fait
/// office de filtre passe-bas, grossier mais reel, et elle est gratuite.
pub fn reechantillonner(entree: &[f32], taux_entree: u32, taux_sortie: u32) -> Vec<f32> {
    if entree.is_empty() || taux_entree == 0 || taux_sortie == 0 {
        return Vec::new();
    }
    if taux_entree == taux_sortie {
        return entree.to_vec();
    }

    let sorties = (entree.len() as u64 * taux_sortie as u64 / taux_entree as u64) as usize;
    let mut resultat = Vec::with_capacity(sorties);

    for indice in 0..sorties {
        let debut = indice as u64 * taux_entree as u64 / taux_sortie as u64;
        let fin = (indice as u64 + 1) * taux_entree as u64 / taux_sortie as u64;
        // ⚠️ En montee de frequence la fenetre est vide : on reprend l'echantillon de depart
        // plutot que de diviser par zero.
        let fin = fin.max(debut + 1).min(entree.len() as u64);
        let debut = debut.min(entree.len() as u64 - 1) as usize;
        let tranche = &entree[debut..fin as usize];
        resultat.push(tranche.iter().sum::<f32>() / tranche.len() as f32);
    }
    resultat
}

/// Ecrit un WAV 16 bits mono, le format que `whisper-cli` lit sans discuter.
pub fn ecrire_wav(chemin: &std::path::Path, echantillons: &[f32], taux: u32) -> Result<(), String> {
    let octets = wav(echantillons, taux);
    std::fs::write(chemin, octets).map_err(|erreur| format!("Audio non écrit : {erreur}"))
}

/// Construit le contenu du fichier WAV. Separe de l'ecriture pour etre testable.
pub fn wav(echantillons: &[f32], taux: u32) -> Vec<u8> {
    let octets_donnees = (echantillons.len() * 2) as u32;
    let mut fichier = Vec::with_capacity(44 + octets_donnees as usize);

    fichier.extend_from_slice(b"RIFF");
    fichier.extend_from_slice(&(36 + octets_donnees).to_le_bytes());
    fichier.extend_from_slice(b"WAVEfmt ");
    fichier.extend_from_slice(&16u32.to_le_bytes()); // taille du bloc de format
    fichier.extend_from_slice(&1u16.to_le_bytes()); // PCM entier
    fichier.extend_from_slice(&1u16.to_le_bytes()); // mono
    fichier.extend_from_slice(&taux.to_le_bytes());
    fichier.extend_from_slice(&(taux * 2).to_le_bytes()); // octets par seconde
    fichier.extend_from_slice(&2u16.to_le_bytes()); // alignement de bloc
    fichier.extend_from_slice(&16u16.to_le_bytes()); // bits par echantillon
    fichier.extend_from_slice(b"data");
    fichier.extend_from_slice(&octets_donnees.to_le_bytes());

    for &echantillon in echantillons {
        // ⚠️ Bornage avant conversion : un flottant hors de [-1, 1] deborderait en `i16` et
        // produirait un craquement, c'est-a-dire du bruit la ou le son etait juste fort.
        let borne = echantillon.clamp(-1.0, 1.0);
        let entier = (borne * i16::MAX as f32) as i16;
        fichier.extend_from_slice(&entier.to_le_bytes());
    }
    fichier
}

/// Amplitude du bip. Assez pour s'entendre par-dessus de la musique, pas assez pour sursauter.
const AMPLITUDE_BIP: f32 = 0.2;

/// Duree du fondu applique aux deux bouts du bip, en millisecondes.
///
/// ⚠️ Sans fondu, une sinusoide qui demarre et s'arrete net produit un **clic** a chaque bout :
/// la discontinuite est un front raide, donc un bruit large bande. Quelques millisecondes
/// suffisent a le faire disparaitre, et s'entendent comme un bip plus propre et non plus court.
const FONDU_MS: u32 = 5;

/// Construit les echantillons d'un bip.
///
/// Fonction PURE, separee de la lecture pour etre testable sans carte son.
pub fn echantillons_de_bip(frequence_hz: u32, duree_ms: u32, taux: u32) -> Vec<f32> {
    if frequence_hz == 0 || duree_ms == 0 || taux == 0 {
        return Vec::new();
    }
    let total = (taux as u64 * duree_ms as u64 / 1000) as usize;
    let fondu = ((taux as u64 * FONDU_MS as u64 / 1000) as usize).min(total / 2);

    (0..total)
        .map(|indice| {
            let phase = indice as f32 * frequence_hz as f32 * std::f32::consts::TAU / taux as f32;
            let enveloppe = if fondu == 0 {
                1.0
            } else if indice < fondu {
                indice as f32 / fondu as f32
            } else if indice >= total - fondu {
                (total - indice) as f32 / fondu as f32
            } else {
                1.0
            };
            phase.sin() * AMPLITUDE_BIP * enveloppe
        })
        .collect()
}

/// Joue un bip et ne rend la main qu'une fois qu'il est fini.
///
/// ⛔ **Bloquant, et c'est voulu pour le bip de DEBUT.** Le bip annonce « j'écoute
/// maintenant » : s'il etait joue pendant que le microphone est ouvert, Oyant
/// **s'enregistrerait lui-meme**. La consequence n'est pas esthetique, elle est fonctionnelle :
/// un bip capte depasse le seuil de silence, donc un appui accidentel declencherait une
/// transcription au lieu d'etre reconnu comme « personne n'a parle ».
///
/// Le prix est une latence egale a la duree du bip, 80 ms par defaut. On la paie volontiers :
/// personne ne commence a parler dans les 80 ms qui suivent son propre appui sur une touche.
pub fn bip(frequence_hz: u32, duree_ms: u32) -> Result<(), String> {
    let hote = cpal::default_host();
    let peripherique = hote
        .default_output_device()
        .ok_or_else(|| "Aucune sortie audio.".to_string())?;
    let configuration = peripherique
        .default_output_config()
        .map_err(|erreur| format!("Sortie audio illisible : {erreur}"))?;

    let taux = configuration.sample_rate();
    let canaux = configuration.channels() as usize;
    let format = configuration.sample_format();
    let echantillons = echantillons_de_bip(frequence_hz, duree_ms, taux);
    if echantillons.is_empty() {
        return Ok(());
    }

    let configuration: cpal::StreamConfig = configuration.into();
    let position = Arc::new(Mutex::new(0usize));

    // ⚠️ Le meme echantillon part sur tous les canaux : un bip mono joue sur une seule oreille
    // s'entend comme une panne de casque.
    let remplir = {
        let echantillons = echantillons.clone();
        let position = position.clone();
        move |sortie: &mut [f32]| {
            let Ok(mut lue) = position.lock() else { return };
            for trame in sortie.chunks_mut(canaux) {
                let valeur = echantillons.get(*lue).copied().unwrap_or(0.0);
                if *lue < echantillons.len() {
                    *lue += 1;
                }
                for canal in trame.iter_mut() {
                    *canal = valeur;
                }
            }
        }
    };

    let sur_erreur = |erreur| eprintln!("sortie audio : {erreur}");
    let flux = match format {
        cpal::SampleFormat::F32 => peripherique.build_output_stream(
            configuration,
            move |sortie: &mut [f32], _| remplir(sortie),
            sur_erreur,
            None,
        ),
        autre => {
            // ⚠️ On ne convertit pas : un bip est un confort, et une conversion de format mal
            // faite s'entend comme un grésillement, donc pire que pas de bip du tout.
            eprintln!("bip ignoré, format de sortie {autre:?} non géré");
            return Ok(());
        }
    }
    .map_err(|erreur| format!("Sortie audio inutilisable : {erreur}"))?;

    flux.play()
        .map_err(|erreur| format!("Bip impossible à jouer : {erreur}"))?;
    std::thread::sleep(std::time::Duration::from_millis(duree_ms as u64));
    Ok(())
}

/// Niveau sonore moyen du passage, entre 0 et 1.
///
/// Sert a distinguer « rien n'a ete dit » de « le microphone n'a rien capte », deux situations
/// que l'utilisateur vit differemment.
pub fn niveau_moyen(echantillons: &[f32]) -> f32 {
    if echantillons.is_empty() {
        return 0.0;
    }
    let somme: f32 = echantillons.iter().map(|valeur| valeur * valeur).sum();
    (somme / echantillons.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⛔ **Le test qui garde la raison d'etre du module.** Whisper n'accepte que 16 kHz, donc un
    /// reechantillonnage qui rend le mauvais nombre d'echantillons rend un son trop rapide ou
    /// trop lent, et la transcription part en charabia sans qu'aucune erreur ne soit levee.
    #[test]
    fn la_duree_est_conservee_par_le_reechantillonnage() {
        // Une seconde a 48 kHz doit rendre une seconde a 16 kHz.
        let entree = vec![0.5f32; 48_000];
        let sortie = reechantillonner(&entree, 48_000, TAUX_MOTEUR);
        assert_eq!(sortie.len(), 16_000);

        // Et le signal constant doit rester constant : une moyenne de 0,5 vaut 0,5.
        assert!(sortie.iter().all(|&valeur| (valeur - 0.5).abs() < 1e-6));
    }

    /// ⛔ **Prend un echantillon sur trois et cette assertion tombe.** C'est la difference entre
    /// un filtre et une decimation : sur un signal qui alterne, la decimation garde une valeur
    /// extreme alors que la moyenne rend le niveau reel.
    #[test]
    fn le_reechantillonnage_moyenne_au_lieu_de_jeter() {
        // Alternance +1 / -1 : la moyenne de chaque fenetre de 3 doit tendre vers 0, alors
        // qu'une decimation rendrait +1 ou -1.
        let entree: Vec<f32> = (0..48_000)
            .map(|indice| if indice % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let sortie = reechantillonner(&entree, 48_000, TAUX_MOTEUR);
        let extreme = sortie.iter().cloned().fold(0.0f32, |pire, valeur| {
            if valeur.abs() > pire {
                valeur.abs()
            } else {
                pire
            }
        });
        assert!(
            extreme < 0.5,
            "le signal n'a pas ete filtre, valeur extreme {extreme}"
        );
    }

    #[test]
    fn un_taux_identique_ne_touche_a_rien() {
        let entree = vec![0.1, -0.2, 0.3];
        assert_eq!(reechantillonner(&entree, 16_000, 16_000), entree);
    }

    #[test]
    fn le_reechantillonnage_supporte_une_entree_vide() {
        assert!(reechantillonner(&[], 48_000, 16_000).is_empty());
        assert!(reechantillonner(&[0.1], 0, 16_000).is_empty());
    }

    /// ⚠️ Garde le piege du canal muet : prendre le premier canal donnerait un silence parfait,
    /// sans erreur, sur les casques dont un canal ne porte rien.
    #[test]
    fn le_repli_en_mono_moyenne_les_canaux() {
        // Gauche porte le son, droite est muette : le mono doit valoir la moitie, pas zero.
        let stereo = vec![1.0, 0.0, 1.0, 0.0];
        assert_eq!(vers_mono(&stereo, 2), vec![0.5, 0.5]);
        // Le mono passe tel quel.
        assert_eq!(vers_mono(&[0.3, 0.4], 1), vec![0.3, 0.4]);
    }

    /// ⚠️ L'en-tete se relit : `whisper-cli` refuse un WAV mal forme, et le message parle alors
    /// du fichier et pas de la frequence, ce qui envoie chercher au mauvais endroit.
    #[test]
    fn l_entete_wav_annonce_16_khz_mono_16_bits() {
        let octets = wav(&[0.0, 0.5, -0.5], TAUX_MOTEUR);
        assert_eq!(&octets[0..4], b"RIFF");
        assert_eq!(&octets[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([octets[22], octets[23]]), 1); // canaux
        assert_eq!(
            u32::from_le_bytes([octets[24], octets[25], octets[26], octets[27]]),
            TAUX_MOTEUR
        );
        assert_eq!(u16::from_le_bytes([octets[34], octets[35]]), 16); // bits
        assert_eq!(octets.len(), 44 + 3 * 2);
    }

    /// ⚠️ Un son fort ne doit pas devenir du bruit : sans bornage, 1.5 deborde en `i16` et
    /// ressort en craquement, qui s'entend comme un defaut de microphone.
    #[test]
    fn un_echantillon_hors_bornes_sature_au_lieu_de_deborder() {
        let octets = wav(&[1.5, -1.5], TAUX_MOTEUR);
        let premier = i16::from_le_bytes([octets[44], octets[45]]);
        let second = i16::from_le_bytes([octets[46], octets[47]]);
        assert_eq!(premier, i16::MAX);
        assert_eq!(second, -i16::MAX);
    }

    /// ⛔ **Garde la touche coincee.** Sans plafond, l'enregistrement grandit jusqu'a saturer la
    /// memoire, puis envoie des heures d'audio au moteur. Le reglage existait et ne pilotait
    /// rien : releve le 2026-09-17, dans l'etape qui venait d'etre livree.
    #[test]
    fn l_enregistrement_s_arrete_au_plafond() {
        // 48 kHz stereo pendant 2 s : deux canaux comptent double.
        assert_eq!(plafond_echantillons(48_000, 2, 2), 192_000);
        assert_eq!(plafond_echantillons(16_000, 1, 120), 1_920_000);

        let mut tampon = Vec::new();
        ajouter(&mut tampon, (0..10).map(|v| v as f32), 4);
        assert_eq!(
            tampon.len(),
            4,
            "le plafond n'a pas borne la premiere salve"
        );

        // Les salves suivantes n'ajoutent plus rien, et ne paniquent pas.
        ajouter(&mut tampon, (0..10).map(|v| v as f32), 4);
        assert_eq!(tampon.len(), 4);
    }

    /// ⚠️ Un plafond nul veut dire « pas de limite », et non « n'enregistre rien » : c'est la
    /// lecture qu'un reglage a zero doit recevoir.
    #[test]
    fn un_plafond_nul_ne_coupe_rien() {
        let mut tampon = Vec::new();
        ajouter(&mut tampon, (0..10).map(|v| v as f32), 0);
        assert_eq!(tampon.len(), 10);
        assert_eq!(plafond_echantillons(48_000, 2, 0), 0);
    }

    /// ⚠️ La duree du bip doit etre celle qu'on a reglee : c'est la seule chose que l'utilisateur
    /// entend, et un bip qui dure autre chose que ce que l'ecran annonce serait un reglage qui
    /// mente sans qu'on puisse le mesurer a l'oreille.
    #[test]
    fn le_bip_dure_ce_qui_est_regle() {
        // 80 ms a 48 kHz.
        assert_eq!(echantillons_de_bip(880, 80, 48_000).len(), 3_840);
        assert_eq!(echantillons_de_bip(440, 1_000, 16_000).len(), 16_000);
    }

    /// ⛔ **Garde le clic.** Une sinusoide qui demarre et s'arrete net cree une discontinuite,
    /// donc un front raide, donc un bruit large bande : ca ne s'entend pas comme un bip plus
    /// court, ca s'entend comme un defaut de materiel. Le fondu doit exister aux DEUX bouts.
    #[test]
    fn le_bip_commence_et_finit_par_un_fondu() {
        let bip = echantillons_de_bip(880, 80, 48_000);
        assert!(!bip.is_empty());
        assert!(
            bip[0].abs() < 0.001,
            "le bip demarre net a {}, il cliquera",
            bip[0]
        );
        let dernier = *bip.last().unwrap();
        assert!(dernier.abs() < 0.01, "le bip s'arrete net a {dernier}");

        // Et le milieu, lui, doit bien porter du signal a pleine amplitude.
        let milieu = bip[bip.len() / 2].abs();
        let maximum = bip.iter().fold(0.0f32, |pire, v| pire.max(v.abs()));
        assert!(
            maximum > AMPLITUDE_BIP * 0.9,
            "amplitude trop faible : {maximum}"
        );
        assert!(
            maximum <= AMPLITUDE_BIP + 1e-6,
            "amplitude trop forte : {maximum}"
        );
        let _ = milieu;
    }

    /// ⚠️ Un reglage a zero ne doit pas produire un bip infini ni une division par zero : il veut
    /// dire « pas de bip », ce que l'appelant lit comme une liste vide.
    #[test]
    fn un_bip_sans_duree_ou_sans_frequence_ne_produit_rien() {
        assert!(echantillons_de_bip(880, 0, 48_000).is_empty());
        assert!(echantillons_de_bip(0, 80, 48_000).is_empty());
        assert!(echantillons_de_bip(880, 80, 0).is_empty());
    }

    /// ⚠️ La frequence reglee doit s'entendre : on compte les passages par zero, qui valent deux
    /// fois la frequence par seconde. Un bip qui sonnerait toujours a la meme hauteur quelle que
    /// soit la valeur reglee serait un reglage decoratif.
    #[test]
    fn la_frequence_reglee_est_celle_qui_sonne() {
        let compter = |frequence: u32| {
            let bip = echantillons_de_bip(frequence, 1_000, 48_000);
            bip.windows(2)
                .filter(|paire| (paire[0] < 0.0) != (paire[1] < 0.0))
                .count()
        };
        // Sur une seconde, environ 2 x la frequence, a quelques unites pres selon les bords.
        let grave = compter(440);
        let aigu = compter(880);
        assert!(
            (grave as i64 - 880).abs() < 10,
            "440 Hz a donne {grave} passages"
        );
        assert!(
            (aigu as i64 - 1760).abs() < 10,
            "880 Hz a donne {aigu} passages"
        );
    }

    /// Ce que la machine expose reellement, entrees et sorties. **A lancer a la main** :
    ///
    /// ```text
    /// cargo test inventaire_audio -- --ignored --nocapture
    /// ```
    ///
    /// ⚠️ Premier reflexe quand un bip ne sonne pas ou qu'un microphone manque : une liste vide
    /// distingue tout de suite « la machine n'a rien » de « notre code cherche mal ».
    #[test]
    #[ignore = "depend du materiel de la machine"]
    fn inventaire_audio() {
        use cpal::traits::HostTrait;
        let hote = cpal::default_host();
        println!("hote : {:?}", hote.id());
        println!("entrees : {:?}", microphones());
        println!(
            "sortie par defaut : {:?}",
            hote.default_output_device().and_then(nom_de)
        );
        let sorties: Vec<String> = hote
            .output_devices()
            .map(|liste| liste.filter_map(nom_de).collect())
            .unwrap_or_default();
        println!("sorties : {sorties:?}");
    }

    /// Fait sonner les bips pour de vrai. **A lancer a la main** :
    ///
    /// ```text
    /// cargo test bip_reel -- --ignored --nocapture
    /// ```
    ///
    /// ⚠️ Ignore par defaut parce qu'il a un effet en dehors du processus, comme le test
    /// d'injection : une integration continue qui ferait sonner la machine de quelqu'un n'est pas
    /// un test, c'est une nuisance. Ce qu'il verifie et qu'aucune fonction pure ne peut couvrir :
    /// que la sortie audio s'ouvre, que le bip s'entend, et que sa duree reelle correspond.
    #[test]
    #[ignore = "fait sonner la machine"]
    fn bip_reel() {
        for (frequence, duree) in [(880u32, 80u32), (440, 200)] {
            let depart = std::time::Instant::now();
            let resultat = bip(frequence, duree);
            let ecoule = depart.elapsed().as_millis() as u32;
            println!("{frequence} Hz / {duree} ms -> {resultat:?}, ecoule {ecoule} ms");
            resultat.expect("le bip doit pouvoir sonner");
            assert!(
                ecoule >= duree,
                "le bip a rendu la main avant la fin : {ecoule} ms pour {duree} ms demandes"
            );
        }
    }

    #[test]
    fn le_niveau_moyen_distingue_le_silence_du_son() {
        assert_eq!(niveau_moyen(&[]), 0.0);
        assert_eq!(niveau_moyen(&[0.0, 0.0]), 0.0);
        assert!(niveau_moyen(&[0.5, -0.5]) > 0.4);
    }

    /// Construit une plage, sans rien ouvrir : c'est tout ce qu'il faut pour rejouer un
    /// peripherique sans microphone U8 sous la main.
    fn plage(
        canaux: u16,
        taux_min: u32,
        taux_max: u32,
        format: cpal::SampleFormat,
    ) -> cpal::SupportedStreamConfigRange {
        cpal::SupportedStreamConfigRange::new(
            canaux,
            // ⚠️ `SampleRate` est un alias de `u32` en cpal 0.18, pas un tuple : pas de `(...)`.
            taux_min,
            taux_max,
            cpal::SupportedBufferSize::Range {
                min: 64,
                max: 4_096,
            },
            format,
        )
    }

    #[test]
    fn tous_les_formats_geres_sont_reellement_geres() {
        // ⛔ Filet contre une regression du genre de celle ci-dessous : si un format est ajoute a
        // `format_gere` sans sa branche de conversion dans `demarrer`, ce test ne l'attrape pas
        // (il ne construit pas de flux), mais au moins les quatre formats geres aujourd'hui sont
        // nommes ici et pas seulement dans le code.
        assert!(format_gere(cpal::SampleFormat::F32));
        assert!(format_gere(cpal::SampleFormat::I16));
        assert!(format_gere(cpal::SampleFormat::U16));
        assert!(format_gere(cpal::SampleFormat::U8));
        assert!(!format_gere(cpal::SampleFormat::I8));
        assert!(!format_gere(cpal::SampleFormat::I32));
    }

    #[test]
    fn un_mono_16khz_en_u8_n_est_plus_choisi_a_tort() {
        // ⛔ **Le bug exact, rejoue sans microphone.** Avant le correctif, cette plage aurait ete
        // retournee telle quelle par la recherche du « cas ideal » : mono, couvre 16 kHz — sans
        // jamais regarder qu'elle est en U8, qu'aucune branche de `demarrer` d'alors ne savait
        // streamer. Resultat reel observe : « Format audio non pris en charge : U8 », a chaque
        // appui du raccourci. Desormais U8 est GERE, donc ce test prouve surtout qu'il est bien
        // choisi plutot qu'ignore — la regression qu'il visait a l'origine est couverte par le
        // suivant, avec un format qu'on ne gere toujours pas.
        let plages = vec![plage(1, 8_000, 48_000, cpal::SampleFormat::U8)];
        let choisie = choisir_configuration(plages.into_iter()).expect("une configuration");
        assert_eq!(choisie.sample_format(), cpal::SampleFormat::U8);
        assert_eq!(choisie.sample_rate(), TAUX_MOTEUR);
    }

    #[test]
    fn un_format_non_gere_n_est_jamais_choisi_meme_mono_16khz() {
        // ⛔ Le vrai filet contre la classe de bug : un format qu'on ne sait PAS convertir
        // (I24, ici) ne doit JAMAIS ressortir de `choisir_configuration`, meme s'il est le seul a
        // proposer du mono 16 kHz. Une plage dans un format gere, moins ideale (stereo, 48 kHz),
        // doit lui etre preferee — mieux convertir que echouer.
        let plages = vec![
            plage(1, 8_000, 48_000, cpal::SampleFormat::I24),
            plage(2, 44_100, 48_000, cpal::SampleFormat::F32),
        ];
        let choisie = choisir_configuration(plages.into_iter()).expect("une configuration");
        assert_eq!(choisie.sample_format(), cpal::SampleFormat::F32);
    }

    #[test]
    fn aucune_plage_geree_rend_aucune_configuration() {
        // ⛔ Le repli existe ensuite cote appelant (`default_input_config`) : cette fonction ne
        // doit pas inventer un choix a partir de rien.
        let plages = vec![plage(1, 8_000, 48_000, cpal::SampleFormat::I24)];
        assert!(choisir_configuration(plages.into_iter()).is_none());
    }
}
