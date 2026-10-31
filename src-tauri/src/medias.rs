//! Mettre la musique en pause pendant la dictee, et la relancer ensuite.
//!
//! ⛔ **Jamais la touche « lecture/pause » du clavier.** C'est la facon la plus repandue de le
//! faire, et c'est une BASCULE : si rien ne joue au moment de dicter, elle LANCE la musique au lieu
//! de la couper. On interroge donc le systeme sur ce qui joue reellement, on ne met en pause que
//! cela, et on ne relance ensuite **que ce qu'on a soi-meme mis en pause**, jamais un lecteur que
//! l'utilisateur avait arrete.
//!
//! - **Windows** : le controle des medias du systeme (`GlobalSystemMediaTransportControls`), celui
//!   qui alimente le panneau de volume. Spotify, les navigateurs (YouTube compris) et la plupart
//!   des lecteurs recents s'y declarent.
//! - **Linux** : MPRIS sur le bus de session, que suivent Spotify, VLC, Firefox et Chromium.
//!
//! ⚠️ **Les appels au systeme tournent dans un fil a part, avec un delai maximal.** Un lecteur qui
//! ne repond pas ne doit jamais retarder l'ouverture du microphone : passe le delai, on dicte sans
//! avoir coupe la musique, ce qui est un moindre mal qu'un raccourci qui semble ne rien faire.

use std::sync::mpsc;
use std::time::Duration;

/// Ce que le systeme dit d'un lecteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lecture {
    EnCours,
    Autre,
}

/// Les lecteurs a mettre en pause : ceux qui jouent, et eux seuls.
///
/// Fonction PURE, testee sans lecteur de musique. Un meme identifiant n'apparait qu'une fois, pour
/// ne pas relancer deux fois le meme lecteur a la reprise.
pub fn a_mettre_en_pause(lecteurs: &[(String, Lecture)]) -> Vec<String> {
    let mut retenus: Vec<String> = Vec::new();
    for (identifiant, lecture) in lecteurs {
        if *lecture == Lecture::EnCours && !retenus.contains(identifiant) {
            retenus.push(identifiant.clone());
        }
    }
    retenus
}

/// Delai au-dela duquel on renonce a couper la musique.
const DELAI: Duration = Duration::from_millis(800);

/// Les lecteurs mis en pause par cette dictee, relances quand la valeur est abandonnee.
///
/// ⚠️ **La reprise passe par `Drop`** : une dictee s'arrete par plusieurs chemins (touche relachee,
/// duree maximale, appui trop bref, erreur), et chacun d'eux oublierait un jour de relancer la
/// musique. Abandonner la session suffit, quel que soit le chemin.
#[derive(Debug, Default)]
pub struct Reprise {
    lecteurs: Vec<String>,
}

impl Reprise {
    #[cfg(test)]
    fn lecteurs(&self) -> &[String] {
        &self.lecteurs
    }
}

impl Drop for Reprise {
    fn drop(&mut self) {
        if self.lecteurs.is_empty() {
            return;
        }
        let lecteurs = std::mem::take(&mut self.lecteurs);
        // Sans attendre : la transcription n'a pas a patienter derriere la reprise de la musique.
        std::thread::spawn(move || {
            for lecteur in &lecteurs {
                if let Err(message) = systeme::relancer(lecteur) {
                    eprintln!("reprise de {lecteur} : {message}");
                }
            }
        });
    }
}

/// Met en pause ce qui joue, et rend de quoi le relancer.
///
/// Ne renvoie jamais d'erreur : une musique qu'on n'a pas pu couper ne doit pas empecher de dicter.
pub fn suspendre() -> Reprise {
    let (envoi, reception) = mpsc::channel();
    std::thread::spawn(move || {
        let resultat = systeme::lecteurs().map(|lecteurs| {
            let mut coupes = Vec::new();
            for lecteur in a_mettre_en_pause(&lecteurs) {
                match systeme::mettre_en_pause(&lecteur) {
                    Ok(()) => coupes.push(lecteur),
                    Err(message) => eprintln!("pause de {lecteur} : {message}"),
                }
            }
            coupes
        });
        let _ = envoi.send(resultat);
    });

    match reception.recv_timeout(DELAI) {
        Ok(Ok(lecteurs)) => Reprise { lecteurs },
        Ok(Err(message)) => {
            eprintln!("pause des medias : {message}");
            Reprise::default()
        }
        Err(_) => {
            eprintln!(
                "pause des medias : pas de reponse du systeme, dictee sans couper la musique"
            );
            Reprise::default()
        }
    }
}

#[cfg(windows)]
mod systeme {
    use super::Lecture;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Gestionnaire,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Statut,
    };
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

    fn erreur(e: windows::core::Error) -> String {
        e.message().to_string()
    }

    /// ⚠️ Appele depuis un fil neuf, qui n'a pas d'appartement : sans `RoInitialize`, les appels
    /// WinRT echouent ou dependent de l'appartement du processus, ce qui marcherait sur une
    /// machine et pas sur une autre.
    fn sessions() -> Result<Vec<Session>, String> {
        // Deja initialise sur ce fil rend une erreur sans consequence : on l'ignore.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let gestionnaire = Gestionnaire::RequestAsync()
            .map_err(erreur)?
            .join()
            .map_err(erreur)?;
        let liste = gestionnaire.GetSessions().map_err(erreur)?;
        Ok(liste.into_iter().collect())
    }

    fn identifiant(session: &Session) -> Option<String> {
        session
            .SourceAppUserModelId()
            .ok()
            .map(|id| id.to_string())
            .filter(|id| !id.is_empty())
    }

    pub fn lecteurs() -> Result<Vec<(String, Lecture)>, String> {
        let mut lecteurs = Vec::new();
        for session in sessions()? {
            let Some(id) = identifiant(&session) else {
                continue;
            };
            let lecture = session
                .GetPlaybackInfo()
                .and_then(|info| info.PlaybackStatus())
                .map(|statut| {
                    if statut == Statut::Playing {
                        Lecture::EnCours
                    } else {
                        Lecture::Autre
                    }
                })
                .unwrap_or(Lecture::Autre);
            lecteurs.push((id, lecture));
        }
        Ok(lecteurs)
    }

    fn agir(lecteur: &str, pause: bool) -> Result<(), String> {
        for session in sessions()? {
            if identifiant(&session).as_deref() != Some(lecteur) {
                continue;
            }
            let operation = if pause {
                session.TryPauseAsync()
            } else {
                session.TryPlayAsync()
            };
            operation.map_err(erreur)?.join().map_err(erreur)?;
        }
        Ok(())
    }

    pub fn mettre_en_pause(lecteur: &str) -> Result<(), String> {
        agir(lecteur, true)
    }

    pub fn relancer(lecteur: &str) -> Result<(), String> {
        agir(lecteur, false)
    }
}

#[cfg(target_os = "linux")]
mod systeme {
    use super::Lecture;
    use zbus::blocking::{Connection, Proxy, fdo::DBusProxy};

    const PREFIXE: &str = "org.mpris.MediaPlayer2.";
    const CHEMIN: &str = "/org/mpris/MediaPlayer2";
    const INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

    fn lecteur<'a>(connexion: &'a Connection, nom: &'a str) -> Result<Proxy<'a>, String> {
        Proxy::new(connexion, nom, CHEMIN, INTERFACE).map_err(|e| e.to_string())
    }

    pub fn lecteurs() -> Result<Vec<(String, Lecture)>, String> {
        let connexion = Connection::session().map_err(|e| e.to_string())?;
        let bus = DBusProxy::new(&connexion).map_err(|e| e.to_string())?;
        let mut lecteurs = Vec::new();
        for nom in bus.list_names().map_err(|e| e.to_string())? {
            let nom = nom.to_string();
            if !nom.starts_with(PREFIXE) {
                continue;
            }
            let statut: String = lecteur(&connexion, &nom)?
                .get_property("PlaybackStatus")
                .unwrap_or_default();
            let lecture = if statut == "Playing" {
                Lecture::EnCours
            } else {
                Lecture::Autre
            };
            lecteurs.push((nom, lecture));
        }
        Ok(lecteurs)
    }

    fn appeler(nom: &str, methode: &str) -> Result<(), String> {
        let connexion = Connection::session().map_err(|e| e.to_string())?;
        lecteur(&connexion, nom)?
            .call_method(methode, &())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn mettre_en_pause(lecteur: &str) -> Result<(), String> {
        appeler(lecteur, "Pause")
    }

    pub fn relancer(lecteur: &str) -> Result<(), String> {
        appeler(lecteur, "Play")
    }
}

/// Plateforme sans controle des medias branche : on ne coupe rien, on ne pretend rien.
#[cfg(not(any(windows, target_os = "linux")))]
mod systeme {
    use super::Lecture;

    pub fn lecteurs() -> Result<Vec<(String, Lecture)>, String> {
        Ok(Vec::new())
    }

    pub fn mettre_en_pause(_: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn relancer(_: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lecteur(id: &str, lecture: Lecture) -> (String, Lecture) {
        (id.to_string(), lecture)
    }

    /// ⛔ **Le cas qui justifie tout le module** : un lecteur en pause ou arrete ne doit pas etre
    /// touche, sinon la reprise le LANCERAIT, exactement le defaut de la touche « lecture/pause ».
    #[test]
    fn seul_ce_qui_joue_est_mis_en_pause() {
        let lecteurs = vec![
            lecteur("Spotify.exe", Lecture::EnCours),
            lecteur("vlc", Lecture::Autre),
            lecteur("Chrome", Lecture::EnCours),
        ];
        assert_eq!(a_mettre_en_pause(&lecteurs), vec!["Spotify.exe", "Chrome"]);
    }

    #[test]
    fn rien_ne_joue_rien_n_est_touche() {
        let lecteurs = vec![lecteur("Spotify.exe", Lecture::Autre)];
        assert!(a_mettre_en_pause(&lecteurs).is_empty());
    }

    /// Deux onglets d'un meme navigateur partagent l'identifiant : le relancer deux fois pourrait
    /// le remettre en pause, puisque certaines applications traitent « lecture » comme une bascule.
    #[test]
    fn un_meme_lecteur_n_est_retenu_qu_une_fois() {
        let lecteurs = vec![
            lecteur("Chrome", Lecture::EnCours),
            lecteur("Chrome", Lecture::EnCours),
        ];
        assert_eq!(a_mettre_en_pause(&lecteurs), vec!["Chrome"]);
    }

    #[test]
    fn une_reprise_vide_ne_fait_rien() {
        let reprise = Reprise::default();
        assert!(reprise.lecteurs().is_empty());
    }

    /// Test sur un VRAI lecteur Windows, que le test lance lui-meme sur le fichier audio designe
    /// par `OYANT_AUDIO_TEST` (un fichier silencieux fait l'affaire).
    ///
    /// ⛔ **Jamais `suspendre()` ici** : il couperait aussi la musique de la personne assise devant
    /// la machine. Seul le lecteur de ce processus est touche, et on refuse de conclure s'il ne joue
    /// pas au depart : un test vert sur un lecteur deja en pause ne prouverait rien.
    #[cfg(windows)]
    #[test]
    #[ignore = "joue un fichier audio ; OYANT_AUDIO_TEST donne son chemin"]
    fn pause_et_reprise_reelles() {
        use windows::Media::Core::MediaSource;
        use windows::Media::Playback::MediaPlayer;
        use windows::Storage::StorageFile;
        use windows::core::HSTRING;

        let chemin = std::env::var("OYANT_AUDIO_TEST").expect("OYANT_AUDIO_TEST");
        let fichier = StorageFile::GetFileFromPathAsync(&HSTRING::from(chemin))
            .and_then(|o| o.join())
            .expect("fichier audio");
        let lecteur_test = MediaPlayer::new().expect("lecteur");
        lecteur_test
            .SetSource(&MediaSource::CreateFromStorageFile(&fichier).expect("source"))
            .expect("source");
        lecteur_test.Play().expect("lecture");
        std::thread::sleep(Duration::from_millis(2500));

        // Le lecteur de ce processus s'annonce sous le nom de l'executable de test.
        let motif = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .expect("nom de l'executable");
        let etat = |id: &str| -> Lecture {
            systeme::lecteurs()
                .expect("liste des lecteurs")
                .into_iter()
                .find(|(i, _)| i == id)
                .map(|(_, l)| l)
                .expect("lecteur disparu")
        };
        let lecteurs = systeme::lecteurs().expect("liste des lecteurs");
        eprintln!("lecteurs vus : {lecteurs:?}");
        let (id, lecture) = lecteurs
            .into_iter()
            .find(|(i, _)| i.contains(&motif))
            .expect("lecteur de test introuvable");
        assert_eq!(
            lecture,
            Lecture::EnCours,
            "le lecteur de test doit jouer au depart"
        );

        systeme::mettre_en_pause(&id).expect("pause");
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(etat(&id), Lecture::Autre, "la pause n'a pas pris");

        systeme::relancer(&id).expect("reprise");
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(etat(&id), Lecture::EnCours, "la reprise n'a pas pris");
    }
}
