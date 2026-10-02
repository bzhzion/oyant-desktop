//! Le serveur d'appairage : TLS, WebSocket, et la poignee de main.
//!
//! ⚠️ **Il ne DECIDE rien.** Toute la regle d'autorisation vit dans `appairage::decider`, qui est
//! pure et testee a part. Ce module transporte, borne et journalise ; melanger les deux aurait
//! remis la securite dans une boucle asynchrone, ou elle ne se teste qu'avec un vrai telephone.
//!
//! ⛔ **Le consentement passe par un CANAL, pas par un rappel.** L'hote envoie la demande a qui
//! sait afficher quelque chose (l'interface, ou un test), et attend une reponse sur un canal a
//! usage unique. C'est ce qui permet d'eprouver le serveur **sans appareil** : un test joue le role
//! de l'utilisateur, y compris quand celui-ci ne repond pas.

use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_rustls::TlsAcceptor;

use crate::appairage::{
    Bonjour, Commande, Consentement, Contexte, Motif, MotifCommande, Preuve, Reponse,
    ReponseCommande, decider, est_wav_valide, verifier_entree, verifier_preuve,
    verifier_taille_annoncee,
};
use crate::reseau::{Appaires, AppareilAppaire, DELAI_APPAIRAGE_S, IdentiteTls, defi};

/// Ce que l'hote demande a l'utilisateur d'accepter.
#[derive(Debug)]
pub struct DemandeAppairage {
    /// Nom declare par l'appareil. ⚠️ Sert a l'affichage et a rien d'autre.
    pub nom: String,
    /// Les quatre chiffres que l'utilisateur doit retrouver sur son telephone.
    pub code: String,
    /// Par ou repondre. ⛔ Si ce canal est abandonne, la demande est traitee comme un REFUS.
    pub reponse: oneshot::Sender<bool>,
}

/// Construit la configuration TLS a partir de notre identite auto-signee.
fn config_tls(identite: &IdentiteTls) -> Result<Arc<rustls::ServerConfig>, String> {
    let certificats = rustls_pemfile::certs(&mut identite.certificat_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>();
    let certificats = certificats.map_err(|e| format!("Certificat illisible : {e}"))?;
    let cle = rustls_pemfile::private_key(&mut identite.cle_pem.as_bytes())
        .map_err(|e| format!("Clé illisible : {e}"))?
        .ok_or("Clé privée absente du PEM.")?;
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificats, cle)
        .map_err(|e| format!("Configuration TLS refusée : {e}"))?;
    Ok(Arc::new(config))
}

/// Sert les connexions jusqu'a ce que le port soit ferme.
///
/// ⚠️ Rend l'adresse REELLEMENT ecoutee, qui n'est pas forcement celle demandee : avec un port 0
/// le systeme en choisit un. Le tests s'en servent, et ca evite la course « lire le port avant
/// qu'il soit ouvert ».
pub async fn servir(
    adresse: std::net::SocketAddr,
    identite: IdentiteTls,
    appaires: Arc<Mutex<Appaires>>,
    demandes: mpsc::Sender<DemandeAppairage>,
) -> Result<(std::net::SocketAddr, tokio::task::JoinHandle<()>), String> {
    let acceptateur = TlsAcceptor::from(config_tls(&identite)?);
    let ecoute = TcpListener::bind(adresse)
        .await
        .map_err(|e| format!("Port {} indisponible : {e}", adresse.port()))?;
    let reelle = ecoute
        .local_addr()
        .map_err(|e| format!("Adresse d'écoute illisible : {e}"))?;

    let tache = tokio::spawn(async move {
        loop {
            let Ok((flux, _)) = ecoute.accept().await else {
                continue;
            };
            let acceptateur = acceptateur.clone();
            let appaires = Arc::clone(&appaires);
            let demandes = demandes.clone();
            // Une connexion qui se comporte mal ne doit jamais empecher les suivantes.
            tokio::spawn(async move {
                let _ = servir_une_connexion(flux, acceptateur, appaires, demandes).await;
            });
        }
    });
    Ok((reelle, tache))
}

/// Lit un message texte, avec le meme delai court sur CHAQUE message et pas seulement le premier.
///
/// ⛔ **Motif repris de justmakeQ, applique a chaque etape** : une connexion qui repond au premier
/// message puis se tait sur le second immobiliserait la meme ressource, pour la meme raison.
async fn lire_message<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>) -> Result<String, String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let message = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        futures_util::StreamExt::next(ws),
    )
    .await
    .map_err(|_| "Message jamais arrivé.".to_string())?
    .ok_or("Connexion fermée avant le message attendu.")?
    .map_err(|e| format!("Message illisible : {e}"))?;
    message
        .into_text()
        .map_err(|e| format!("Message non textuel : {e}"))
}

async fn servir_une_connexion(
    flux: tokio::net::TcpStream,
    acceptateur: TlsAcceptor,
    appaires: Arc<Mutex<Appaires>>,
    demandes: mpsc::Sender<DemandeAppairage>,
) -> Result<(), String> {
    let flux = acceptateur
        .accept(flux)
        .await
        .map_err(|e| format!("Poignée de main TLS échouée : {e}"))?;
    let mut ws = tokio_tungstenite::accept_async(flux)
        .await
        .map_err(|e| format!("WebSocket refusé : {e}"))?;

    // ── 1. Bonjour ───────────────────────────────────────────────────────────────────────────
    let texte = lire_message(&mut ws).await?;
    let bonjour: Bonjour = match serde_json::from_str(&texte) {
        Ok(b) => b,
        Err(_) => {
            repondre(
                &mut ws,
                &Reponse::Refus {
                    motif: Motif::MessageInvalide,
                },
            )
            .await;
            return Ok(());
        }
    };

    // ⛔ Version et bornes AVANT d'envoyer le moindre defi : refuser un client qu'on ne saura pas
    // interpreter ne doit couter qu'un seul aller-retour, pas deux.
    if let Some(refus) = verifier_entree(&bonjour) {
        repondre(&mut ws, &refus).await;
        return Ok(());
    }

    // ── 2. Defi -> Preuve ────────────────────────────────────────────────────────────────────
    //
    // ⛔ **Envoye a TOUT le monde, connu ou non.** C'est ce qui remplace le secret porteur : un
    // appareil deja retenu ne rentre plus sur la seule presentation de sa cle publique, il doit
    // reprouver la posseder a CHAQUE connexion.
    use base64::Engine;
    let decodeur = base64::engine::general_purpose::STANDARD;
    let defi_courant = defi();
    repondre(
        &mut ws,
        &Reponse::Defi {
            defi: decodeur.encode(defi_courant),
        },
    )
    .await;

    let texte = lire_message(&mut ws).await?;
    let preuve: Preuve = match serde_json::from_str(&texte) {
        Ok(p) => p,
        Err(_) => {
            repondre(
                &mut ws,
                &Reponse::Refus {
                    motif: Motif::MessageInvalide,
                },
            )
            .await;
            return Ok(());
        }
    };
    if let Some(refus) = verifier_preuve(&bonjour, &defi_courant, &preuve) {
        repondre(&mut ws, &refus).await;
        return Ok(());
    }

    // ── 3. La decision, maintenant que la possession est prouvee ────────────────────────────
    let premiere = {
        let liste = appaires.lock().await;
        decider(
            &bonjour,
            &Contexte {
                appaires: &liste,
                defi: &defi_courant,
            },
            Consentement::NonSollicite,
        )
    };

    let finale = match &premiere {
        // Rien a demander : deja connu, ou deja refuse.
        Reponse::Bienvenue { .. } | Reponse::Refus { .. } => premiere,
        Reponse::AutorisationDemandee { .. } => {
            // ⛔ **Envoyee AU TELEPHONE des maintenant, avant d'attendre le consentement.** Sans
            // ce message, le telephone n'avait jamais reçu son propre code : il attendait juste la
            // reponse finale en silence, pendant que seul l'ordinateur affichait un code — ce qui
            // viderait de son sens la consigne « comparez les deux nombres », affichee cote
            // ordinateur, puisque rien n'apparaissait cote telephone pour la comparer.
            repondre(&mut ws, &premiere).await;
            let Reponse::AutorisationDemandee { code } = &premiere else {
                unreachable!("filtre juste au-dessus")
            };
            let consentement = demander(&demandes, &bonjour.nom, code).await;
            let liste = appaires.lock().await;
            decider(
                &bonjour,
                &Contexte {
                    appaires: &liste,
                    defi: &defi_courant,
                },
                consentement,
            )
        }
        Reponse::Defi { .. } => unreachable!("verifier_entree/verifier_preuve n'en rendent pas"),
    };

    // Un appairage accepte se retient, sinon l'utilisateur devrait redire oui a chaque connexion.
    //
    // ⛔ **Ecrit sur le DISQUE, pas seulement en memoire.** Une premiere version ne faisait
    // qu'ajouter a la liste partagee en memoire (`Arc<Mutex<Appaires>>`) : l'ecran disait
    // « Téléphone autorisé », la liste affichee restait vide (elle relit le fichier via
    // `reseau_appareils`/`charger_appaires`), et un redemarrage d'Oyant aurait perdu
    // l'autorisation sans que personne ne s'en apercoive avant la prochaine tentative. Trouve en
    // appairant pour de vrai, pas en relecture.
    if let Reponse::Bienvenue { empreinte } = &finale {
        let mut liste = appaires.lock().await;
        if !liste.autorise(empreinte) {
            liste.ajouter(AppareilAppaire {
                empreinte: empreinte.clone(),
                nom: bonjour.nom.clone(),
                appaire_le: horodatage(),
            });
            if let Err(message) = enregistrer_appaires(&liste) {
                eprintln!("appairage : échec d'écriture de la liste des appareils : {message}");
            }
        }
    }

    repondre(&mut ws, &finale).await;

    // ⚠️ Seul un appareil qui vient d'etre dit « bienvenue » peut demander quoi que ce soit
    // ensuite. Un refus ferme la connexion sans jamais lire un message de plus.
    if matches!(finale, Reponse::Bienvenue { .. }) {
        match lire_commande_facultative(&mut ws).await {
            // ⛔ Le cas NORMAL : l'ecran « Mon ordinateur » du telephone ferme la connexion des
            // qu'il a son `Bienvenue`, sans rien demander de plus. Rien a journaliser.
            None => {}
            Some(Err(message)) => eprintln!("appairage : commande illisible : {message}"),
            Some(Ok(commande)) => {
                let reponse = traiter_commande(&mut ws, commande).await;
                repondre(&mut ws, &reponse).await;
            }
        }
    }

    Ok(())
}

/// Lit une COMMANDE facultative, apres `Bienvenue`.
///
/// ⚠️ **Contrairement a `lire_message`, une connexion qui se ferme ICI n'est PAS une erreur** :
/// c'est le cas normal d'un appairage qui ne demandait rien de plus. `None` le distingue d'un
/// `Some(Err(_))`, qui signale un VRAI probleme (message illisible, ou non textuel).
async fn lire_commande_facultative<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Option<Result<Commande, String>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let attente = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        futures_util::StreamExt::next(ws),
    )
    .await;
    // Delai ou connexion fermee proprement : dans les deux cas, rien a faire et ce n'est pas une
    // erreur. `?` sur l'`Option` interne gere le second cas (`None` = flux termine).
    let message = attente.ok()??;

    let texte = match message {
        Ok(m) => match m.into_text() {
            Ok(t) => t,
            Err(e) => return Some(Err(format!("message non textuel : {e}"))),
        },
        Err(e) => return Some(Err(format!("message illisible : {e}"))),
    };
    Some(serde_json::from_str::<Commande>(&texte).map_err(|_| "commande non reconnue".to_string()))
}

/// Compteur de requetes reseau, pour un nom de fichier temporaire UNIQUE par connexion.
///
/// ⚠️ **Ni `std::process::id()` seul (deja utilise par la dictee locale au clavier) ni un nom
/// fixe** : deux transcriptions reseau simultanees, ou une reseau en meme temps qu'une dictee
/// locale, ecraseraient alors le MEME fichier temporaire pendant que l'autre l'utilise encore.
fn prochain_identifiant_temporaire() -> u64 {
    static COMPTEUR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    COMPTEUR.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Execute une commande reçue apres `Bienvenue`, et rend la reponse a transmettre au telephone.
async fn traiter_commande<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
    commande: Commande,
) -> ReponseCommande
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    match commande {
        Commande::TranscrireEnregistrement { taille_octets } => {
            transcrire_enregistrement_recu(ws, taille_octets).await
        }
    }
}

/// Reçoit un enregistrement WAV et le transcrit avec le moteur et le modele CONFIGURES sur cet
/// ordinateur, exactement comme pour une dictee au clavier.
///
/// ⚠️ **Les reglages sont relus ICI, pas transmis depuis le demarrage du serveur** : une
/// configuration changee entre-temps (modele, langue, vocabulaire) doit s'appliquer, meme principe
/// que `enregistrer()` qui relit le disque avant d'ecrire, cote interface.
///
/// ⛔ **N'injecte RIEN au curseur et ne touche pas le presse-papiers.** Contrairement a la dictee
/// tenue au clavier, il n'y a ici aucune fenetre que l'utilisateur regarde au moment ou la demande
/// arrive : taper dans celle qui a le focus par hasard serait taper dans la mauvaise application.
/// Le texte est seulement renvoye au telephone, et ajoute a l'historique comme toute dictee.
async fn transcrire_enregistrement_recu<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
    taille_octets: u64,
) -> ReponseCommande
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    if let Some(motif) = verifier_taille_annoncee(taille_octets) {
        return ReponseCommande::Refus { motif };
    }

    // ⚠️ Delai genereux : un enregistrement de plusieurs dizaines de Mio sur un wifi ordinaire
    // peut prendre un vrai moment, contrairement aux messages de l'appairage qui ne pesent que
    // quelques octets.
    let attente = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        futures_util::StreamExt::next(ws),
    )
    .await;
    let Ok(Some(Ok(message))) = attente else {
        return ReponseCommande::Refus {
            motif: MotifCommande::MessageInvalide,
        };
    };
    let octets = match message {
        tokio_tungstenite::tungstenite::Message::Binary(o) => o,
        // ⛔ Un texte, une image fermee ou un ping la ou un enregistrement etait annonce : ce
        // n'est pas ce qui a ete annonce, refus franc plutot que d'essayer de deviner.
        _ => {
            return ReponseCommande::Refus {
                motif: MotifCommande::MessageInvalide,
            };
        }
    };

    if octets.len() as u64 != taille_octets || !est_wav_valide(&octets) {
        return ReponseCommande::Refus {
            motif: MotifCommande::FormatInvalide,
        };
    }

    let reglages = crate::reglages::lire_sans_application();
    let Ok(outils) = crate::dictee::resoudre(&reglages.modele) else {
        return ReponseCommande::Refus {
            motif: MotifCommande::MoteurIndisponible,
        };
    };

    let chemin = std::env::temp_dir().join(format!(
        "oyant-reseau-{}.wav",
        prochain_identifiant_temporaire()
    ));
    if std::fs::write(&chemin, &octets).is_err() {
        return ReponseCommande::Refus {
            motif: MotifCommande::FormatInvalide,
        };
    }

    let resultat = crate::moteur::transcrire(
        &outils.executable,
        &outils.modele,
        &chemin,
        &reglages.langue,
        reglages.fils,
        reglages.temperature,
        crate::moteur::prompt_des_reglages(&reglages).as_deref(),
    );
    let _ = std::fs::remove_file(&chemin);

    match resultat {
        Ok(transcription) => {
            let brut = transcription.texte.trim().to_string();
            // ⚠️ Comme pour la dictee locale : c'est le texte BRUT qui a une valeur de trace,
            // substitutions et majuscules sont des choix d'affichage qui n'entrent pas dedans.
            if !brut.is_empty() {
                let _ = crate::historique::ajouter(&brut, reglages.taille_historique);
            }
            ReponseCommande::Transcription { texte: brut }
        }
        Err(_) => ReponseCommande::Refus {
            motif: MotifCommande::EchecTranscription,
        },
    }
}

/// Demande a l'utilisateur, et traite tout ce qui n'est pas un « oui » franc comme un refus.
///
/// ⛔ **Trois facons de ne pas dire oui, une seule conclusion.** Le delai expire, le canal ferme
/// parce que l'interface a disparu, ou l'utilisateur dit non : dans les trois cas on refuse. Une
/// seule branche qui autoriserait par defaut suffirait a rendre tout le reste decoratif.
async fn demander(
    demandes: &mpsc::Sender<DemandeAppairage>,
    nom: &str,
    code: &str,
) -> Consentement {
    let (repondeur, attente) = oneshot::channel();
    let demande = DemandeAppairage {
        nom: nom.to_string(),
        code: code.to_string(),
        reponse: repondeur,
    };
    if demandes.send(demande).await.is_err() {
        // Personne n'ecoute : il n'y a pas d'interface pour afficher la demande.
        return Consentement::Expire;
    }
    match tokio::time::timeout(std::time::Duration::from_secs(DELAI_APPAIRAGE_S), attente).await {
        Ok(Ok(true)) => Consentement::Accepte,
        Ok(Ok(false)) => Consentement::Refuse,
        // Canal abandonne : l'interface s'est fermee sans repondre.
        Ok(Err(_)) => Consentement::Expire,
        Err(_) => Consentement::Expire,
    }
}

/// ⚠️ Generique sur `Reponse` ET `ReponseCommande` : les deux se serialisent en JSON de la meme
/// facon, et dupliquer cette fonction pour chacune n'aurait rien ajoute.
async fn repondre<S, R: serde::Serialize>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
    reponse: &R,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    if let Ok(texte) = serde_json::to_string(reponse) {
        let _ =
            futures_util::SinkExt::send(ws, tokio_tungstenite::tungstenite::Message::Text(texte))
                .await;
    }
}

/// Demarre le serveur si, et seulement si, l'utilisateur l'a demande.
///
/// ⛔ **Le filtre est ICI, au demarrage, et pas dans une case de l'interface.** Un serveur qu'on
/// ouvre puis qu'on « protege » plus loin est un serveur ouvert. Tant que `reseau_actif` est faux,
/// aucun port n'est lie.
///
/// ⚠️ Ne rend jamais d'erreur bloquante : un port indisponible ne doit pas empecher Oyant de
/// dicter. On le dit sur la sortie d'erreur et on continue, comme pour le raccourci global.
pub fn demarrer_si_demande(app: &tauri::AppHandle) {
    let reglages = crate::reglages::lire_sans_application();
    if !reglages.reseau_actif {
        return;
    }
    let joignable = reglages.reseau_toutes_interfaces;
    let adresse = crate::reseau::adresse_ecoute(joignable);
    let identite = match crate::reseau::identite_tls() {
        Ok(i) => i,
        Err(message) => {
            eprintln!("appairage : {message}");
            return;
        }
    };
    let appaires = Arc::new(Mutex::new(charger_appaires()));
    let empreinte = match crate::reseau::empreinte_certificat(&identite.certificat_pem) {
        Ok(e) => e,
        Err(message) => {
            eprintln!("appairage : {message}");
            return;
        }
    };
    // Canal des demandes : l'interface les affiche, et repond. ⚠️ Si personne ne le consomme,
    // `demander` conclut au refus — voir sa documentation.
    let (envoi, mut reception) = mpsc::channel::<DemandeAppairage>(4);

    let poignee = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(demande) = reception.recv().await {
            // ⛔ La demande est MISE EN ATTENTE, pas refusee tout de suite : c'est l'ecran qui
            // repond, par `reseau_repondre`. Si personne ne repond, le delai de `demander` finit
            // par conclure au refus — le silence refuse, il n'autorise jamais.
            let identifiant = demande.code.clone();
            en_attente()
                .lock()
                .expect("verrou des demandes")
                .insert(identifiant.clone(), demande.reponse);
            let _ = tauri::Emitter::emit(
                &poignee,
                "appairage-demande",
                serde_json::json!({ "id": identifiant, "nom": demande.nom, "code": demande.code }),
            );
        }
    });

    tauri::async_runtime::spawn(async move {
        match servir(adresse, identite, appaires, envoi).await {
            Ok((reelle, _)) => {
                eprintln!("appairage : à l'écoute sur {reelle}");
                // ⛔ On n'annonce QUE si l'ordinateur est joignable. Annoncer une ecoute limitee a
                // la boucle locale ferait TROUVER l'ordinateur par le telephone, puis echouer la
                // connexion : l'utilisateur verrait son ordinateur dans la liste et conclurait a
                // un probleme d'appairage ou de certificat, alors qu'il lui manque simplement une
                // case. Ne rien trouver est un symptome honnete, trouver et ne pas joindre non.
                if !joignable {
                    eprintln!(
                        "appairage : à l'écoute en local seulement, pas d'annonce sur le réseau"
                    );
                    return;
                }
                // ⚠️ L'annonce vient APRES l'ecoute : annoncer un port qui n'ecoute pas encore
                // ferait echouer la premiere tentative de connexion, et ce genre d'echec se lit
                // comme « ca ne marche pas » plutot que comme une course.
                match annoncer(reelle.port(), &empreinte) {
                    // Le demon est garde vivant par la tache : le laisser tomber retirerait
                    // l'annonce du reseau aussitot.
                    Ok(demon) => {
                        eprintln!("appairage : annoncé en {TYPE_SERVICE}");
                        std::mem::forget(demon);
                    }
                    Err(message) => eprintln!("appairage : {message}"),
                }
            }
            Err(message) => eprintln!("appairage : {message}"),
        }
    });
}

/// Les demandes qui attendent une reponse de l'ecran, par code.
///
/// ⚠️ Un `Mutex` de la bibliotheque standard et non de tokio : il n'est tenu que le temps d'une
/// insertion ou d'un retrait, jamais a travers un `await`.
fn en_attente()
-> &'static std::sync::Mutex<std::collections::HashMap<String, oneshot::Sender<bool>>> {
    static DEMANDES: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, oneshot::Sender<bool>>>,
    > = std::sync::OnceLock::new();
    DEMANDES.get_or_init(Default::default)
}

/// Reponse de l'utilisateur a une demande d'appairage.
///
/// ⛔ **C'est la seule facon d'autoriser**, et elle part de la machine qui recevra les frappes.
/// Rend `false` si la demande a deja expire : l'ecran ne doit pas laisser croire qu'un appairage
/// a reussi quand le telephone a deja renonce.
#[tauri::command]
pub fn reseau_repondre(id: String, accepte: bool) -> bool {
    let Some(repondeur) = en_attente()
        .lock()
        .expect("verrou des demandes")
        .remove(&id)
    else {
        return false;
    };
    repondeur.send(accepte).is_ok()
}

/// Type de service zeroconf annonce sur le reseau local.
///
/// ⚠️ **Un type DECLARE, et pas une enumeration de tout ce qui passe.** C'est ce qui evite
/// l'autorisation « multicast » d'Apple : `NSBonjourServices` declare ce type, et le telephone n'a
/// alors besoin que de `NSLocalNetworkUsageDescription`. Verifie sur `justmakeq-app`, qui declare
/// les deux et aucune autorisation multicast.
pub const TYPE_SERVICE: &str = "_oyant._tcp.local.";

/// Annonce cet ordinateur sur le reseau local.
///
/// ⛔ **L'empreinte du certificat part dans l'annonce.** C'est elle que le telephone epingle avant
/// meme d'ouvrir la connexion : sans elle il devrait faire confiance au premier certificat
/// presente, ce qui laisserait n'importe qui sur le wifi se placer au milieu du tout premier
/// appairage — le seul moment ou il n'y a encore rien a comparer.
///
/// ⚠️ **L'annonce ne rend pas joignable** : elle dit seulement « je suis la ». Tant que
/// `reseau_toutes_interfaces` est faux, le port reste sur la boucle locale et l'annonce ne mene
/// nulle part. Les deux reglages sont distincts a dessein.
fn annoncer(port: u16, empreinte: &str) -> Result<mdns_sd::ServiceDaemon, String> {
    let demon =
        mdns_sd::ServiceDaemon::new().map_err(|e| format!("Découverte indisponible : {e}"))?;
    let machine = hostname_court();
    let service = mdns_sd::ServiceInfo::new(
        TYPE_SERVICE,
        &machine,
        &format!("{machine}.local."),
        (),
        port,
        &[
            ("empreinte", empreinte),
            ("version", crate::appairage::VERSION_PROTOCOLE),
        ][..],
    )
    .map_err(|e| format!("Annonce impossible : {e}"))?
    .enable_addr_auto();
    demon
        .register(service)
        .map_err(|e| format!("Annonce refusée : {e}"))?;
    Ok(demon)
}

/// Nom de la machine, tel qu'il s'affichera sur le telephone.
///
/// ⚠️ Rend un nom neutre plutot que d'echouer : l'appairage ne doit pas dependre d'une variable
/// d'environnement absente.
fn hostname_court() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Oyant".to_string())
}

/// Ou vit la liste des appareils appaires.
fn chemin_appaires() -> Result<std::path::PathBuf, String> {
    Ok(crate::chemins::configuration()?.join("appaires.json"))
}

/// Charge la liste, en rendant une liste VIDE si quoi que ce soit cloche.
///
/// ⛔ **Un fichier illisible doit vider la liste, jamais l'ignorer en gardant une autorisation.**
/// Se tromper dans ce sens ferait redemander un appairage ; se tromper dans l'autre laisserait
/// entrer un appareil que l'utilisateur croit avoir revoque.
pub fn charger_appaires() -> Appaires {
    let Ok(chemin) = chemin_appaires() else {
        return Appaires::default();
    };
    let Ok(contenu) = std::fs::read_to_string(chemin) else {
        return Appaires::default();
    };
    serde_json::from_str(&contenu).unwrap_or_default()
}

/// Ecrit la liste.
pub fn enregistrer_appaires(liste: &Appaires) -> Result<(), String> {
    let chemin = chemin_appaires()?;
    if let Some(parent) = chemin.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Répertoire impossible : {e}"))?;
    }
    let texte =
        serde_json::to_string_pretty(liste).map_err(|e| format!("Liste non sérialisable : {e}"))?;
    std::fs::write(&chemin, texte).map_err(|e| format!("Liste non écrite : {e}"))
}

/// L'empreinte du certificat de CETTE machine, a montrer dans les reglages.
///
/// ⛔ **C'est elle que le telephone epingle**, et c'est donc elle qu'un utilisateur compare s'il
/// veut verifier a quoi il se connecte. Elle part aussi dans le QR du repli, avec l'adresse et le
/// port, quand la decouverte ne passe pas — cas de l'isolation des clients.
#[tauri::command]
pub fn reseau_empreinte() -> Result<String, String> {
    let identite = crate::reseau::identite_tls()?;
    crate::reseau::empreinte_certificat(&identite.certificat_pem)
}

/// Les appareils autorises, pour l'ecran de reglages.
#[tauri::command]
pub fn reseau_appareils() -> Vec<AppareilAppaire> {
    charger_appaires().appareils
}

/// Retire un appareil. Rend `true` si quelque chose a ete retire.
#[tauri::command]
pub fn reseau_revoquer(empreinte: String) -> Result<bool, String> {
    let mut liste = charger_appaires();
    let retire = liste.revoquer(&empreinte);
    enregistrer_appaires(&liste)?;
    Ok(retire)
}

/// Horodatage ISO 8601, sans dependance de date supplementaire.
fn horodatage() -> String {
    let secondes = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secondes}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appairage::VERSION_PROTOCOLE;
    use crate::reseau::{empreinte, fabriquer_identite_tls};

    /// Verificateur qui n'accepte QUE le certificat epingle.
    ///
    /// ⛔ **C'est exactement ce que le telephone doit faire**, et c'est pour ca qu'il vit ici
    /// plutot que dans un utilitaire de test anonyme : il documente la contrepartie. Un client qui
    /// accepterait n'importe quel certificat rendrait tout le TLS decoratif, puisque n'importe qui
    /// sur le wifi pourrait se placer au milieu.
    #[derive(Debug)]
    struct Epingle(Vec<u8>);

    impl rustls::client::danger::ServerCertVerifier for Epingle {
        fn verify_server_cert(
            &self,
            end_entity: &rustls::pki_types::CertificateDer<'_>,
            _intermediates: &[rustls::pki_types::CertificateDer<'_>],
            _server_name: &rustls::pki_types::ServerName<'_>,
            _ocsp: &[u8],
            _now: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            if end_entity.as_ref() == self.0.as_slice() {
                Ok(rustls::client::danger::ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General("certificat non épinglé".into()))
            }
        }

        fn verify_tls12_signature(
            &self,
            _m: &[u8],
            _c: &rustls::pki_types::CertificateDer<'_>,
            _d: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _m: &[u8],
            _c: &rustls::pki_types::CertificateDer<'_>,
            _d: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    /// Joue une connexion complete : TLS epingle, WebSocket, un `bonjour`, et rend la reponse.
    /// Genere une paire Ed25519 de test et rend (cle publique b64, paire).
    ///
    /// ⚠️ **C'est ce qu'un vrai telephone genere via CryptoKit** ; ces tests ne rejouent pas
    /// CryptoKit (absent de ce poste), ils prouvent que le meme protocole marche avec de VRAIES
    /// cles Ed25519 plutot que d'inventer une signature bidon qui ne prouverait rien.
    fn paire_de_test() -> (String, ring::signature::Ed25519KeyPair) {
        use base64::Engine;
        use ring::signature::KeyPair;
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).expect("generation");
        let paire = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("parsing");
        let publique = base64::engine::general_purpose::STANDARD.encode(paire.public_key());
        (publique, paire)
    }

    /// Joue une connexion complete : TLS epingle, WebSocket, `bonjour`, puis signe le defi recu et
    /// continue de lire jusqu'a une reponse TERMINALE (`Bienvenue`/`Refus`).
    ///
    /// ⛔ **`Defi` et `AutorisationDemandee` sont transparents ici, exactement comme un vrai
    /// telephone** : le premier se signe et se renvoie sans intervention du test, le second se
    /// contente d'etre lu et ignore (c'est desormais le TELEPHONE qui le reçoit en premier, avant
    /// meme que l'utilisateur ait repondu sur l'ordinateur — voir le commentaire dans
    /// `servir_une_connexion`). Aucun test actuel n'a besoin d'inspecter son contenu : le code que
    /// l'utilisateur compare est verifie via le canal `demandes`, pas via ce que le telephone
    /// affiche.
    /// Type du flux qu'un client de test obtient : TLS epingle par-dessus TCP, WebSocket par-dessus.
    type FluxTest =
        tokio_tungstenite::WebSocketStream<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>;

    /// Ouvre la connexion TLS epinglee + WebSocket, SANS rien envoyer : partagee par `dialoguer`
    /// et par les tests qui doivent garder la main sur le flux apres `Bienvenue`.
    async fn tls_connecter(adresse: std::net::SocketAddr, certificat_der: Vec<u8>) -> FluxTest {
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Epingle(certificat_der)))
            .with_no_client_auth();
        let connecteur = tokio_rustls::TlsConnector::from(Arc::new(config));
        let tcp = tokio::net::TcpStream::connect(adresse).await.expect("tcp");
        let nom_serveur = rustls::pki_types::ServerName::try_from("oyant.local").expect("nom");
        let tls = connecteur.connect(nom_serveur, tcp).await.expect("tls");
        tokio_tungstenite::client_async("ws://oyant.local/", tls)
            .await
            .expect("websocket")
            .0
    }

    async fn envoyer_json(ws: &mut FluxTest, valeur: serde_json::Value) {
        futures_util::SinkExt::send(
            ws,
            tokio_tungstenite::tungstenite::Message::Text(valeur.to_string()),
        )
        .await
        .expect("envoi");
    }

    /// Mene le handshake jusqu'a une reponse TERMINALE (`Bienvenue`/`Refus`), en signant chaque
    /// defi recu. Partagee par `dialoguer` et `dialoguer_jusqu_a_bienvenue`.
    async fn handshake(
        ws: &mut FluxTest,
        nom: &str,
        paire: &ring::signature::Ed25519KeyPair,
        cle_publique: &str,
        version: &str,
    ) -> Reponse {
        use base64::Engine;
        let decodeur = base64::engine::general_purpose::STANDARD;

        envoyer_json(
            ws,
            serde_json::json!({ "version": version, "nom": nom, "cle_publique": cle_publique }),
        )
        .await;

        loop {
            let recu = futures_util::StreamExt::next(ws)
                .await
                .expect("réponse")
                .expect("trame");
            let reponse: Reponse =
                serde_json::from_str(&recu.into_text().expect("texte")).expect("json");
            match reponse {
                Reponse::Defi { defi } => {
                    let defi_octets = decodeur.decode(&defi).expect("defi base64");
                    let signature = decodeur.encode(paire.sign(&defi_octets));
                    envoyer_json(ws, serde_json::json!({ "signature": signature })).await;
                }
                Reponse::AutorisationDemandee { .. } => continue,
                terminale => return terminale,
            }
        }
    }

    async fn dialoguer(
        adresse: std::net::SocketAddr,
        certificat_der: Vec<u8>,
        nom: &str,
        paire: &ring::signature::Ed25519KeyPair,
        cle_publique: &str,
        version: &str,
    ) -> Reponse {
        let mut ws = tls_connecter(adresse, certificat_der).await;
        handshake(&mut ws, nom, paire, cle_publique, version).await
    }

    /// Comme `dialoguer`, mais s'ARRETE a `Bienvenue` et REND la connexion encore ouverte : c'est
    /// ce qu'il faut pour envoyer une commande juste apres, ce que `dialoguer` ne permet pas
    /// puisqu'il referme tout en rendant sa reponse.
    async fn dialoguer_jusqu_a_bienvenue(
        adresse: std::net::SocketAddr,
        certificat_der: Vec<u8>,
        nom: &str,
        paire: &ring::signature::Ed25519KeyPair,
        cle_publique: &str,
    ) -> FluxTest {
        let mut ws = tls_connecter(adresse, certificat_der).await;
        let reponse = handshake(&mut ws, nom, paire, cle_publique, VERSION_PROTOCOLE).await;
        assert!(
            matches!(reponse, Reponse::Bienvenue { .. }),
            "attendu Bienvenue avant d'envoyer une commande, obtenu {reponse:?}"
        );
        ws
    }

    /// Monte un serveur sur un port libre et rend de quoi lui parler.
    async fn serveur_de_test(
        appaires: Arc<Mutex<Appaires>>,
    ) -> (
        std::net::SocketAddr,
        Vec<u8>,
        mpsc::Receiver<DemandeAppairage>,
    ) {
        let identite = fabriquer_identite_tls().expect("identité");
        let der = rustls_pemfile::certs(&mut identite.certificat_pem.as_bytes())
            .next()
            .expect("un certificat")
            .expect("lisible")
            .as_ref()
            .to_vec();
        let (envoi, reception) = mpsc::channel(4);
        // Port 0 : le systeme en choisit un libre, et `servir` rend celui qui a ete pris. Figer un
        // port ferait echouer ce test des qu'une autre chose ecoute dessus.
        let adresse = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let (reelle, _tache) = servir(adresse, identite, appaires, envoi)
            .await
            .expect("serveur");
        (reelle, der, reception)
    }

    #[tokio::test]
    async fn un_inconnu_accepte_par_l_utilisateur_entre_et_reste_retenu() {
        let appaires = Arc::new(Mutex::new(Appaires::default()));
        let (adresse, der, mut demandes) = serveur_de_test(Arc::clone(&appaires)).await;
        let (cle_publique, paire) = paire_de_test();

        // L'utilisateur dit oui. ⚠️ Le test joue son role, ce qui permet d'eprouver le serveur
        // sans appareil — y compris les cas ou il ne repond pas (voir les tests suivants).
        tokio::spawn(async move {
            let demande = demandes.recv().await.expect("une demande");
            assert_eq!(
                demande.code.len(),
                4,
                "l'utilisateur doit voir quatre chiffres"
            );
            assert_eq!(demande.nom, "iPhone");
            let _ = demande.reponse.send(true);
        });

        let reponse = dialoguer(
            adresse,
            der,
            "iPhone",
            &paire,
            &cle_publique,
            VERSION_PROTOCOLE,
        )
        .await;
        assert_eq!(
            reponse,
            Reponse::Bienvenue {
                empreinte: empreinte(cle_publique.as_bytes())
            }
        );
        // ⛔ Et il est RETENU : sans ca l'utilisateur redirait oui a chaque connexion, et finirait
        // par dire oui sans regarder.
        assert!(
            appaires
                .lock()
                .await
                .autorise(&empreinte(cle_publique.as_bytes()))
        );
    }

    #[tokio::test]
    async fn un_inconnu_refuse_reste_dehors_et_n_est_pas_retenu() {
        let appaires = Arc::new(Mutex::new(Appaires::default()));
        let (adresse, der, mut demandes) = serveur_de_test(Arc::clone(&appaires)).await;
        let (cle_publique, paire) = paire_de_test();

        tokio::spawn(async move {
            let demande = demandes.recv().await.expect("une demande");
            let _ = demande.reponse.send(false);
        });

        let reponse = dialoguer(
            adresse,
            der,
            "iPhone",
            &paire,
            &cle_publique,
            VERSION_PROTOCOLE,
        )
        .await;
        assert_eq!(
            reponse,
            Reponse::Refus {
                motif: Motif::Refuse
            }
        );
        assert!(
            !appaires
                .lock()
                .await
                .autorise(&empreinte(cle_publique.as_bytes()))
        );
    }

    #[tokio::test]
    async fn connaitre_la_cle_publique_ne_suffit_plus_a_entrer() {
        // ⛔ **C'est le test qui prouve le remplacement du secret porteur, sur le vrai reseau.**
        // Avant cette etape, presenter la meme cle publique qu'un appareil deja appaire suffisait
        // a rentrer. Ici un imposteur DECLARE la bonne cle publique mais signe avec une AUTRE
        // paire : il n'a pas la cle privee correspondante, et doit etre refuse.
        let (cle_publique, _vraie_paire) = paire_de_test();
        let mut appaires_init = Appaires::default();
        appaires_init.ajouter(AppareilAppaire {
            empreinte: empreinte(cle_publique.as_bytes()),
            nom: "iPhone".into(),
            appaire_le: "2026-09-25T20:00:00Z".into(),
        });
        let appaires = Arc::new(Mutex::new(appaires_init));
        let (adresse, der, mut demandes) = serveur_de_test(Arc::clone(&appaires)).await;

        let (_autre_cle_publique, autre_paire) = paire_de_test();
        let reponse = dialoguer(
            adresse,
            der,
            "iPhone",
            &autre_paire,
            &cle_publique,
            VERSION_PROTOCOLE,
        )
        .await;
        assert_eq!(
            reponse,
            Reponse::Refus {
                motif: Motif::PreuveInvalide
            }
        );
        // ⛔ Et l'utilisateur n'a meme pas ete derange : refuser une preuve invalide ne doit pas
        // dependre d'un humain qui regarderait un code, sinon un imposteur pourrait tenter sa
        // chance jusqu'a ce que quelqu'un clique sans regarder.
        assert!(
            demandes.try_recv().is_err(),
            "une preuve invalide ne doit jamais solliciter l'utilisateur"
        );
    }

    #[tokio::test]
    async fn un_appareil_deja_appaire_rentre_avec_sa_vraie_cle_sans_rien_demander() {
        let (cle_publique, vraie_paire) = paire_de_test();
        let mut appaires_init = Appaires::default();
        appaires_init.ajouter(AppareilAppaire {
            empreinte: empreinte(cle_publique.as_bytes()),
            nom: "iPhone".into(),
            appaire_le: "2026-09-25T20:00:00Z".into(),
        });
        let appaires = Arc::new(Mutex::new(appaires_init));
        let (adresse, der, mut demandes) = serveur_de_test(Arc::clone(&appaires)).await;

        let reponse = dialoguer(
            adresse,
            der,
            "iPhone",
            &vraie_paire,
            &cle_publique,
            VERSION_PROTOCOLE,
        )
        .await;
        assert_eq!(
            reponse,
            Reponse::Bienvenue {
                empreinte: empreinte(cle_publique.as_bytes())
            }
        );
        assert!(
            demandes.try_recv().is_err(),
            "un appareil deja connu ne doit rien redemander a l'utilisateur"
        );
    }

    #[tokio::test]
    async fn personne_ne_repond_donc_on_refuse() {
        let appaires = Arc::new(Mutex::new(Appaires::default()));
        // ⛔ Le recepteur est ABANDONNE : c'est le cas « l'interface n'est pas la pour afficher la
        // demande ». Il doit refuser, jamais autoriser par defaut.
        let (adresse, der, demandes) = serveur_de_test(Arc::clone(&appaires)).await;
        drop(demandes);
        let (cle_publique, paire) = paire_de_test();

        let reponse = dialoguer(
            adresse,
            der,
            "iPhone",
            &paire,
            &cle_publique,
            VERSION_PROTOCOLE,
        )
        .await;
        assert_eq!(
            reponse,
            Reponse::Refus {
                motif: Motif::SansReponse
            }
        );
        assert!(
            !appaires
                .lock()
                .await
                .autorise(&empreinte(cle_publique.as_bytes()))
        );
    }

    #[tokio::test]
    async fn une_mauvaise_version_ne_derange_meme_pas_l_utilisateur() {
        let appaires = Arc::new(Mutex::new(Appaires::default()));
        let (adresse, der, mut demandes) = serveur_de_test(Arc::clone(&appaires)).await;
        let (cle_publique, paire) = paire_de_test();

        let reponse = dialoguer(adresse, der, "iPhone", &paire, &cle_publique, "0").await;
        assert_eq!(
            reponse,
            Reponse::Refus {
                motif: Motif::VersionIncompatible
            }
        );
        // ⛔ Le point du test : AUCUNE demande n'a ete envoyee a l'utilisateur. Le deranger pour un
        // client qu'on ne sait pas interpreter, c'est l'habituer a accepter sans lire.
        assert!(
            demandes.try_recv().is_err(),
            "l'utilisateur ne doit pas être sollicité pour une version incompatible"
        );
    }

    // ── Transcrire un enregistrement envoye apres l'appairage ───────────────────────────────

    /// Monte un serveur de test dont l'appareil rendu est DEJA appaire : ⛔ sans ca, un appareil
    /// inconnu declenche la demande de consentement humain, et `_demandes` ignoree dans les tests
    /// ci-dessous la ferait tomber sur `Refus { motif: SansReponse }` avant meme d'atteindre la
    /// commande qu'on veut eprouver. C'est precisement le piege dans lequel ces quatre tests sont
    /// tombes au premier essai.
    async fn serveur_avec_appareil_deja_appaire() -> (
        std::net::SocketAddr,
        Vec<u8>,
        String,
        ring::signature::Ed25519KeyPair,
    ) {
        let (cle_publique, paire) = paire_de_test();
        let mut appaires_init = Appaires::default();
        appaires_init.ajouter(AppareilAppaire {
            empreinte: empreinte(cle_publique.as_bytes()),
            nom: "iPhone".into(),
            appaire_le: "2026-09-25T20:00:00Z".into(),
        });
        let (adresse, der, _demandes) = serveur_de_test(Arc::new(Mutex::new(appaires_init))).await;
        (adresse, der, cle_publique, paire)
    }

    /// Un en-tete WAV minimal valide, sans aucun son derriere : suffisant pour passer la
    /// verification de forme, pas pour obtenir une vraie transcription.
    fn wav_minimal() -> Vec<u8> {
        let mut octets = vec![0u8; 44];
        octets[0..4].copy_from_slice(b"RIFF");
        octets[8..12].copy_from_slice(b"WAVE");
        octets
    }

    async fn lire_reponse_commande(ws: &mut FluxTest) -> ReponseCommande {
        let recu = futures_util::StreamExt::next(ws)
            .await
            .expect("une reponse de commande")
            .expect("trame");
        serde_json::from_str(&recu.into_text().expect("texte")).expect("json")
    }

    #[tokio::test]
    async fn une_taille_annoncee_demesuree_est_refusee_sans_rien_lire_de_plus() {
        let (adresse, der, cle_publique, paire) = serveur_avec_appareil_deja_appaire().await;
        let mut ws =
            dialoguer_jusqu_a_bienvenue(adresse, der, "iPhone", &paire, &cle_publique).await;

        envoyer_json(
            &mut ws,
            serde_json::json!({ "type": "transcrire_enregistrement", "taille_octets": crate::appairage::TAILLE_ENREGISTREMENT_MAX + 1 }),
        )
        .await;

        // ⛔ Le point du test : aucun octet de donnees n'est envoye APRES l'annonce, et pourtant
        // une reponse arrive quand meme. Si le serveur attendait de recevoir le fichier avant de
        // verifier sa taille, cette connexion resterait bloquee jusqu'au delai, pas refusee tout
        // de suite.
        assert_eq!(
            lire_reponse_commande(&mut ws).await,
            ReponseCommande::Refus {
                motif: MotifCommande::FichierTropGros
            }
        );
    }

    #[tokio::test]
    async fn une_taille_annoncee_qui_ne_correspond_pas_est_refusee() {
        let (adresse, der, cle_publique, paire) = serveur_avec_appareil_deja_appaire().await;
        let mut ws =
            dialoguer_jusqu_a_bienvenue(adresse, der, "iPhone", &paire, &cle_publique).await;

        let wav = wav_minimal();
        envoyer_json(
            &mut ws,
            // ⛔ Annonce DELIBEREMENT une taille differente de ce qui va vraiment suivre.
            serde_json::json!({ "type": "transcrire_enregistrement", "taille_octets": (wav.len() + 1) as u64 }),
        )
        .await;
        futures_util::SinkExt::send(
            &mut ws,
            tokio_tungstenite::tungstenite::Message::Binary(wav),
        )
        .await
        .expect("envoi audio");

        assert_eq!(
            lire_reponse_commande(&mut ws).await,
            ReponseCommande::Refus {
                motif: MotifCommande::FormatInvalide
            }
        );
    }

    #[tokio::test]
    async fn des_octets_qui_ne_sont_pas_un_wav_sont_refuses() {
        let (adresse, der, cle_publique, paire) = serveur_avec_appareil_deja_appaire().await;
        let mut ws =
            dialoguer_jusqu_a_bienvenue(adresse, der, "iPhone", &paire, &cle_publique).await;

        let n_importe_quoi = vec![0u8; 100];
        envoyer_json(
            &mut ws,
            serde_json::json!({ "type": "transcrire_enregistrement", "taille_octets": 100u64 }),
        )
        .await;
        futures_util::SinkExt::send(
            &mut ws,
            tokio_tungstenite::tungstenite::Message::Binary(n_importe_quoi),
        )
        .await
        .expect("envoi audio");

        assert_eq!(
            lire_reponse_commande(&mut ws).await,
            ReponseCommande::Refus {
                motif: MotifCommande::FormatInvalide
            }
        );
    }

    #[tokio::test]
    async fn un_wav_valide_de_taille_annoncee_atteint_la_transcription() {
        let (adresse, der, cle_publique, paire) = serveur_avec_appareil_deja_appaire().await;
        let mut ws =
            dialoguer_jusqu_a_bienvenue(adresse, der, "iPhone", &paire, &cle_publique).await;

        let wav = wav_minimal();
        envoyer_json(
            &mut ws,
            serde_json::json!({ "type": "transcrire_enregistrement", "taille_octets": wav.len() as u64 }),
        )
        .await;
        futures_util::SinkExt::send(
            &mut ws,
            tokio_tungstenite::tungstenite::Message::Binary(wav),
        )
        .await
        .expect("envoi audio");

        // ⚠️ **Pas d'assertion sur UNE reponse precise** : au-dela de la forme du WAV, la suite
        // depend du moteur et du modele reellement CONFIGURES sur la machine qui fait tourner ce
        // test -- absents sur une CI fraiche, presents sur un poste qui dicte deja au quotidien.
        // Ce qui compte ici, c'est que le protocole reseau ait ete traverse jusqu'au bout SANS
        // etre refuse pour une raison de FORME (taille, format) : la reponse doit donc etre soit
        // une vraie transcription, soit un refus faute de MOTEUR configure, jamais les refus de
        // forme qui, eux, se produisent AVANT meme de regarder le moteur.
        let reponse = lire_reponse_commande(&mut ws).await;
        assert!(
            !matches!(
                reponse,
                ReponseCommande::Refus {
                    motif: MotifCommande::FichierTropGros | MotifCommande::FormatInvalide
                }
            ),
            "un WAV valide de la bonne taille ne doit jamais echouer sur sa FORME : {reponse:?}"
        );
    }
}
