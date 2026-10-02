//! Le protocole d'appairage, et la decision qui l'accompagne.
//!
//! ⚠️ **Separe de `reseau.rs` a dessein.** Celui-la porte les primitives (empreintes, code visuel,
//! certificat), celui-ci porte les messages et **la decision d'autoriser ou non**. Les melanger
//! aurait donne un fichier ou la regle de securite se cherche entre deux fonctions utilitaires.
//!
//! ⛔ **Toute la decision vit dans `decider`, une fonction PURE.** C'est delibere : une regle
//! d'autorisation enfouie dans une boucle asynchrone ne se teste qu'avec un vrai reseau et un vrai
//! telephone, donc en pratique ne se teste pas. Ici elle se prouve par des tests, y compris les
//! cas qu'on n'a pas envie de fabriquer a la main — mauvaise version, appareil revoque, silence de
//! l'utilisateur.

use serde::{Deserialize, Serialize};

use crate::reseau::{Appaires, code_visuel, empreinte, verifier_signature};

/// Version du protocole, comparee STRICTEMENT.
///
/// ⛔ **Refusee avant meme la demande d'autorisation**, motif repris de `justmakeQ`. Laisser entrer
/// un client d'une autre version puis decouvrir qu'il se comporte de travers, c'est avoir deja
/// demande a l'utilisateur d'autoriser quelque chose qu'on ne sait pas interpreter.
///
/// ⛔ **Passee a `"2"` avec l'ajout de la preuve de possession.** Un telephone de l'ancienne
/// version n'enverrait jamais de `Preuve` : le laisser entrer serait bloque plus tard par un delai
/// d'attente illisible plutot que par un refus clair et immediat. Bureau et mobile montent
/// ensemble ; un ancien de l'un ou l'autre cote est proprement rejete, jamais mal interprete.
pub const VERSION_PROTOCOLE: &str = "2";

/// Ce que le telephone envoie en premier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bonjour {
    pub version: String,
    /// Nom affiche. ⚠️ Declare par le client, donc **jamais** utilise pour autoriser.
    pub nom: String,
    /// Cle publique Ed25519 du telephone, en base64. ⚠️ **Depuis la version 2, c'est une VRAIE
    /// cle publique et non plus un secret porteur** : elle n'autorise rien tant que le telephone
    /// n'a pas prouve posseder la cle privee correspondante (voir `Preuve` et `verifier_preuve`).
    pub cle_publique: String,
}

/// Ce que le telephone envoie en second, une fois qu'il a recu le defi de l'hote.
///
/// ⛔ **C'est elle qui remplace le secret porteur.** Avant la version 2, presenter la meme
/// `cle_publique` deux fois suffisait a rentrer. Desormais il faut, a CHAQUE connexion, signer un
/// defi neuf avec la cle privee : une signature capturee sur un defi ne vaut plus rien sur un
/// autre, donc l'observer une fois (par ex. dans un journal mal garde) ne donne aucun acces futur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preuve {
    /// Signature Ed25519 du defi recu, en base64.
    pub signature: String,
}

/// Ce que l'hote repond, et qui resume la decision prise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reponse {
    /// Premier message de l'hote : le defi a signer. ⚠️ Envoye des que `Bonjour` passe les
    /// bornes et la version — avant meme de savoir si l'appareil est deja connu — pour que la
    /// preuve de possession soit exigee dans TOUS les cas, connu ou non.
    Defi { defi: String },
    /// Appareil deja appaire : la session s'ouvre sans rien demander a personne.
    Bienvenue { empreinte: String },
    /// Appareil inconnu : l'utilisateur doit accepter SUR L'ORDINATEUR, avec ce code sous les yeux.
    AutorisationDemandee { code: String },
    /// Refus, avec sa raison. ⚠️ La raison est destinee a l'UTILISATEUR, pas au client : elle doit
    /// nommer ce qu'il peut faire, pas ce que le serveur a decide.
    Refus { motif: Motif },
}

/// Pourquoi une connexion a ete refusee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motif {
    /// Le telephone et l'ordinateur ne parlent pas la meme version.
    VersionIncompatible,
    /// Le premier message n'est pas conforme, ou dépasse les bornes.
    MessageInvalide,
    /// La signature du defi ne correspond pas a la cle publique declaree — ou n'a pas pu etre
    /// lue. ⚠️ Regroupe volontairement les deux cas (voir `reseau::verifier_signature`) : une
    /// raison plus precise renseignerait un attaquant sur ce qui a echoue, sans lui donner acces.
    PreuveInvalide,
    /// Personne n'a repondu a la demande d'autorisation.
    ///
    /// ⛔ **Le silence REFUSE.** Une demande qui expire en autorisant serait un appairage qu'on
    /// obtient en attendant que l'utilisateur s'eloigne de son clavier.
    SansReponse,
    /// L'utilisateur a dit non.
    Refuse,
}

/// Ce que l'hote sait au moment de decider.
pub struct Contexte<'a> {
    pub appaires: &'a Appaires,
    pub defi: &'a [u8],
}

/// Ce que l'utilisateur a repondu, quand on a eu besoin de le lui demander.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consentement {
    /// Personne n'a encore ete sollicite : l'appareil etait deja connu.
    NonSollicite,
    Accepte,
    Refuse,
    /// Le delai est passe sans reponse.
    Expire,
}

/// Bornes sur le premier message.
///
/// ⛔ **Un champ non borne est une facon de faire tomber le serveur avant toute authentification.**
/// Les longueurs viennent de l'usage : un nom d'appareil tient largement en 100 caracteres, une
/// cle publique encodee en 1000.
const NOM_MAX: usize = 100;
const CLE_MAX: usize = 1000;

/// Borne sur le second message (la signature). ⚠️ Une signature Ed25519 encodee en base64 tient
/// en une centaine de caracteres ; 200 laisse de la marge sans rouvrir la porte a un champ
/// arbitrairement long avant toute verification cryptographique.
const SIGNATURE_MAX: usize = 200;

/// Verifie la version et les bornes de `Bonjour`, et REND LA REPONSE DE REFUS s'il y a lieu.
///
/// ⛔ **Extrait de `decider` pour etre appele AVANT d'envoyer le defi.** Refuser une mauvaise
/// version doit couter un aller-retour, pas deux : ca evite de faire signer quoi que ce soit a un
/// client qu'on ne saura de toute facon pas interpreter. `decider` continue de l'appeler aussi,
/// en defense en profondeur — les deux appels sont idempotents.
pub fn verifier_entree(bonjour: &Bonjour) -> Option<Reponse> {
    // ⛔ La version AVANT tout le reste : on ne demande pas a l'utilisateur d'autoriser un client
    // qu'on ne saura pas interpreter ensuite.
    if bonjour.version != VERSION_PROTOCOLE {
        return Some(Reponse::Refus {
            motif: Motif::VersionIncompatible,
        });
    }
    if bonjour.nom.is_empty()
        || bonjour.nom.len() > NOM_MAX
        || bonjour.cle_publique.is_empty()
        || bonjour.cle_publique.len() > CLE_MAX
    {
        return Some(Reponse::Refus {
            motif: Motif::MessageInvalide,
        });
    }
    None
}

/// Verifie la preuve de possession (la signature du defi), et REND LA REPONSE DE REFUS si elle ne
/// correspond pas.
///
/// ⛔ **C'est ELLE qui remplace le secret porteur.** Sans cet appel, un `Bonjour` portant une cle
/// publique deja connue suffirait a entrer (exactement le defaut de justmakeQ) : desormais, il
/// faut prouver posseder la cle privee correspondante, a chaque connexion, avant meme de savoir
/// si l'appareil est deja autorise. Appelee AVANT tout le reste de `decider`.
pub fn verifier_preuve(bonjour: &Bonjour, defi: &[u8], preuve: &Preuve) -> Option<Reponse> {
    if preuve.signature.is_empty() || preuve.signature.len() > SIGNATURE_MAX {
        return Some(Reponse::Refus {
            motif: Motif::MessageInvalide,
        });
    }
    if verifier_signature(&bonjour.cle_publique, defi, &preuve.signature) {
        None
    } else {
        Some(Reponse::Refus {
            motif: Motif::PreuveInvalide,
        })
    }
}

/// La decision, et rien d'autre.
///
/// ⚠️ **Pure** : pas d'entree/sortie, pas d'horloge, pas de reseau. C'est ce qui la rend
/// testable — et c'est la seule raison pour laquelle les cas penibles (version fausse, silence de
/// l'utilisateur, appareil revoque entre-temps) sont couverts.
///
/// ⛔ **N'est appelee, cote serveur, qu'APRES que `verifier_preuve` ait deja accepte la
/// signature.** Cette fonction ne verifie plus elle-meme la possession de la cle : ce n'est pas un
/// oubli, c'est le decoupage voulu (verifier_entree -> verifier_preuve -> decider), chacun pur et
/// teste separement.
pub fn decider(bonjour: &Bonjour, contexte: &Contexte, consentement: Consentement) -> Reponse {
    if let Some(refus) = verifier_entree(bonjour) {
        return refus;
    }

    let empreinte_client = empreinte(bonjour.cle_publique.as_bytes());

    // Deja connu : rien a demander, et surtout rien a redemander a chaque connexion.
    if contexte.appaires.autorise(&empreinte_client) {
        return Reponse::Bienvenue {
            empreinte: empreinte_client,
        };
    }

    match consentement {
        Consentement::NonSollicite => Reponse::AutorisationDemandee {
            code: code_visuel(&empreinte_client, contexte.defi),
        },
        Consentement::Accepte => Reponse::Bienvenue {
            empreinte: empreinte_client,
        },
        Consentement::Refuse => Reponse::Refus {
            motif: Motif::Refuse,
        },
        // ⛔ Le silence refuse. Voir `Motif::SansReponse`.
        Consentement::Expire => Reponse::Refus {
            motif: Motif::SansReponse,
        },
    }
}

// ── Apres `Bienvenue` : la premiere chose qu'on peut demander ───────────────────────────────
//
// ⚠️ Un appareil qui se contente de s'appairer (l'ecran « Mon ordinateur » du telephone) ferme la
// connexion des qu'il a son `Bienvenue`, et c'est le cas NORMAL : aucune commande n'arrive alors,
// et ce n'est pas une erreur. Voir `serveur::lire_commande_facultative`, qui distingue ce silence
// d'un vrai probleme.

/// Taille maximale d'un enregistrement envoye par le telephone.
///
/// ⚠️ **Verifiee AVANT de recevoir le moindre octet** : le telephone l'annonce dans `Commande`, ce
/// qui permet de refuser un fichier deraisonnable sans avoir a le recevoir en entier d'abord.
/// ~200 Mio couvre plus d'une heure a 16 kHz mono 16 bits (le format que le telephone envoie deja
/// pour sa propre transcription sur l'appareil), avec de la marge.
pub const TAILLE_ENREGISTREMENT_MAX: u64 = 200 * 1024 * 1024;

/// Ce qu'un appareil appaire peut demander, une fois `Bienvenue` reçu.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Commande {
    /// Transcrire un enregistrement deja fait sur le telephone. ⚠️ L'audio suit en un message
    /// BINAIRE separe, envoye juste apres celui-ci : ce message-ci ne fait qu'annoncer sa taille.
    TranscrireEnregistrement { taille_octets: u64 },
}

/// Ce que l'hote repond a une commande.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReponseCommande {
    Transcription { texte: String },
    Refus { motif: MotifCommande },
}

/// Pourquoi une commande a ete refusee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotifCommande {
    /// La commande elle-meme est illisible, ou ses bornes sont depassees.
    MessageInvalide,
    /// La taille annoncee depasse `TAILLE_ENREGISTREMENT_MAX`, ou vaut zero.
    FichierTropGros,
    /// Les octets reçus ne sont pas un WAV valide, ou ne correspondent pas a la taille annoncee.
    FormatInvalide,
    /// Aucun moteur ou modele n'est configure sur cet ordinateur.
    MoteurIndisponible,
    /// Le moteur a echoue sur ce fichier precis.
    EchecTranscription,
}

/// Verifie la taille ANNONCEE d'un enregistrement, avant d'en recevoir le moindre octet.
///
/// Fonction PURE : aucune raison de recevoir un fichier pour decouvrir qu'on va le refuser.
pub fn verifier_taille_annoncee(taille_octets: u64) -> Option<MotifCommande> {
    if taille_octets == 0 || taille_octets > TAILLE_ENREGISTREMENT_MAX {
        Some(MotifCommande::FichierTropGros)
    } else {
        None
    }
}

/// Verifie la forme MINIMALE d'un fichier WAV : l'en-tete RIFF/WAVE, rien de plus.
///
/// ⚠️ **Ne verifie PAS le sous-format** (PCM, 16 kHz, mono) : `whisper-cli` le fait lui-meme et le
/// dit clairement s'il refuse, dupliquer ce parsing ici n'apporterait rien de plus qu'un second
/// endroit a desynchroniser du premier.
pub fn est_wav_valide(octets: &[u8]) -> bool {
    octets.len() >= 44 && &octets[0..4] == b"RIFF" && &octets[8..12] == b"WAVE"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseau::AppareilAppaire;

    fn bonjour(nom: &str, cle: &str) -> Bonjour {
        Bonjour {
            version: VERSION_PROTOCOLE.to_string(),
            nom: nom.to_string(),
            cle_publique: cle.to_string(),
        }
    }

    fn contexte<'a>(appaires: &'a Appaires, defi: &'a [u8]) -> Contexte<'a> {
        Contexte { appaires, defi }
    }

    #[test]
    fn une_mauvaise_version_est_refusee_avant_de_demander_quoi_que_ce_soit() {
        let vides = Appaires::default();
        let mut b = bonjour("iPhone", "cle");
        b.version = "0".into();
        let r = decider(&b, &contexte(&vides, b"defi"), Consentement::NonSollicite);
        // ⛔ Le point du test : PAS d'AutorisationDemandee. Sinon on aurait sollicite
        // l'utilisateur pour un client qu'on ne sait pas interpreter.
        assert_eq!(
            r,
            Reponse::Refus {
                motif: Motif::VersionIncompatible
            }
        );
    }

    #[test]
    fn un_champ_hors_bornes_est_refuse() {
        let vides = Appaires::default();
        for b in [
            bonjour("", "cle"),
            bonjour(&"x".repeat(NOM_MAX + 1), "cle"),
            bonjour("iPhone", ""),
            bonjour("iPhone", &"x".repeat(CLE_MAX + 1)),
        ] {
            let r = decider(&b, &contexte(&vides, b"defi"), Consentement::NonSollicite);
            assert_eq!(
                r,
                Reponse::Refus {
                    motif: Motif::MessageInvalide
                },
                "un champ non borne fait tomber le serveur avant toute authentification"
            );
        }
    }

    #[test]
    fn un_inconnu_declenche_une_demande_avec_son_code() {
        let vides = Appaires::default();
        let b = bonjour("iPhone", "cle");
        let r = decider(&b, &contexte(&vides, b"defi"), Consentement::NonSollicite);
        match r {
            Reponse::AutorisationDemandee { code } => {
                assert_eq!(code.len(), 4);
                assert_eq!(code, code_visuel(&empreinte(b"cle"), b"defi"));
            }
            autre => panic!("attendu une demande, obtenu {autre:?}"),
        }
    }

    #[test]
    fn un_appareil_deja_appaire_entre_sans_rien_demander() {
        let mut liste = Appaires::default();
        liste.ajouter(AppareilAppaire {
            empreinte: empreinte(b"cle"),
            nom: "iPhone".into(),
            appaire_le: "2026-09-25T20:00:00Z".into(),
        });
        let b = bonjour("iPhone", "cle");
        let r = decider(&b, &contexte(&liste, b"defi"), Consentement::NonSollicite);
        assert_eq!(
            r,
            Reponse::Bienvenue {
                empreinte: empreinte(b"cle")
            }
        );
    }

    #[test]
    fn un_appareil_revoque_redevient_un_inconnu() {
        let mut liste = Appaires::default();
        liste.ajouter(AppareilAppaire {
            empreinte: empreinte(b"cle"),
            nom: "iPhone".into(),
            appaire_le: "2026-09-25T20:00:00Z".into(),
        });
        liste.revoquer(&empreinte(b"cle"));
        let b = bonjour("iPhone", "cle");
        let r = decider(&b, &contexte(&liste, b"defi"), Consentement::NonSollicite);
        // ⛔ Une revocation qui laisserait entrer serait pire que pas de revocation du tout :
        // l'utilisateur croirait avoir coupe l'acces.
        assert!(matches!(r, Reponse::AutorisationDemandee { .. }));
    }

    #[test]
    fn le_silence_refuse() {
        let vides = Appaires::default();
        let b = bonjour("iPhone", "cle");
        let r = decider(&b, &contexte(&vides, b"defi"), Consentement::Expire);
        // ⛔ Sinon l'appairage s'obtient en attendant que l'utilisateur s'eloigne de son clavier.
        assert_eq!(
            r,
            Reponse::Refus {
                motif: Motif::SansReponse
            }
        );
    }

    #[test]
    fn un_refus_de_l_utilisateur_est_un_refus() {
        let vides = Appaires::default();
        let b = bonjour("iPhone", "cle");
        assert_eq!(
            decider(&b, &contexte(&vides, b"defi"), Consentement::Refuse),
            Reponse::Refus {
                motif: Motif::Refuse
            }
        );
    }

    #[test]
    fn le_nom_declare_n_ouvre_aucune_porte() {
        // Un appareil est appaire. Un autre se presente avec EXACTEMENT le meme nom, mais une
        // autre cle : il doit rester un inconnu. C'est le defaut de justmakeQ qu'on refuse.
        let mut liste = Appaires::default();
        liste.ajouter(AppareilAppaire {
            empreinte: empreinte(b"vraie-cle"),
            nom: "iPhone de painteau".into(),
            appaire_le: "2026-09-25T20:00:00Z".into(),
        });
        let imposteur = bonjour("iPhone de painteau", "autre-cle");
        let r = decider(
            &imposteur,
            &contexte(&liste, b"defi"),
            Consentement::NonSollicite,
        );
        assert!(
            matches!(r, Reponse::AutorisationDemandee { .. }),
            "un nom identique ne doit jamais suffire a entrer"
        );
    }

    /// Genere une paire Ed25519 de test et rend (cle publique b64, paire).
    fn paire_de_test() -> (String, ring::signature::Ed25519KeyPair) {
        use base64::Engine;
        use ring::signature::KeyPair;
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).expect("generation");
        let paire = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("parsing");
        let publique = base64::engine::general_purpose::STANDARD.encode(paire.public_key());
        (publique, paire)
    }

    fn signer(paire: &ring::signature::Ed25519KeyPair, message: &[u8]) -> Preuve {
        use base64::Engine;
        Preuve {
            signature: base64::engine::general_purpose::STANDARD.encode(paire.sign(message)),
        }
    }

    #[test]
    fn une_preuve_valide_ne_refuse_rien() {
        let (_publique, paire) = paire_de_test();
        let b = bonjour("iPhone", &_publique);
        let preuve = signer(&paire, b"defi-du-serveur");
        assert_eq!(verifier_preuve(&b, b"defi-du-serveur", &preuve), None);
    }

    #[test]
    fn une_preuve_qui_ne_correspond_pas_a_la_cle_declaree_est_refusee() {
        // ⛔ Le coeur du remplacement du secret porteur : declarer la cle publique de QUELQU'UN
        // D'AUTRE, sans en posseder la cle privee, doit echouer ici — precisement le cas qu'un
        // secret porteur ne pouvait pas empecher (presenter la meme chaine suffisait).
        let (publique_a, _paire_a) = paire_de_test();
        let (_publique_b, paire_b) = paire_de_test();
        let b = bonjour("iPhone", &publique_a);
        let preuve = signer(&paire_b, b"defi-du-serveur");
        assert_eq!(
            verifier_preuve(&b, b"defi-du-serveur", &preuve),
            Some(Reponse::Refus {
                motif: Motif::PreuveInvalide
            })
        );
    }

    #[test]
    fn une_signature_capturee_ne_vaut_rien_sur_un_autre_defi() {
        // ⛔ C'est precisement ce qu'un secret porteur ne peut pas offrir : une valeur observee
        // une fois (par ex. dans un journal) ne redonne pas acces, puisque le prochain defi sera
        // different et la signature ne portera plus dessus.
        let (publique, paire) = paire_de_test();
        let b = bonjour("iPhone", &publique);
        let preuve = signer(&paire, b"ancien-defi");
        assert_eq!(
            verifier_preuve(&b, b"nouveau-defi", &preuve),
            Some(Reponse::Refus {
                motif: Motif::PreuveInvalide
            })
        );
    }

    #[test]
    fn une_signature_vide_ou_trop_longue_est_un_message_invalide() {
        let (publique, _paire) = paire_de_test();
        let b = bonjour("iPhone", &publique);
        assert_eq!(
            verifier_preuve(
                &b,
                b"defi",
                &Preuve {
                    signature: String::new()
                }
            ),
            Some(Reponse::Refus {
                motif: Motif::MessageInvalide
            })
        );
        assert_eq!(
            verifier_preuve(
                &b,
                b"defi",
                &Preuve {
                    signature: "x".repeat(SIGNATURE_MAX + 1)
                }
            ),
            Some(Reponse::Refus {
                motif: Motif::MessageInvalide
            })
        );
    }

    #[test]
    fn une_taille_annoncee_raisonnable_passe() {
        assert_eq!(verifier_taille_annoncee(1), None);
        assert_eq!(verifier_taille_annoncee(TAILLE_ENREGISTREMENT_MAX), None);
    }

    #[test]
    fn une_taille_annoncee_nulle_ou_demesuree_est_refusee() {
        // ⛔ Zero n'est pas un cas limite anodin : un fichier de zero octet ne contient aucun
        // en-tete WAV possible, autant le refuser avant meme d'ouvrir une connexion pour rien.
        assert_eq!(
            verifier_taille_annoncee(0),
            Some(MotifCommande::FichierTropGros)
        );
        assert_eq!(
            verifier_taille_annoncee(TAILLE_ENREGISTREMENT_MAX + 1),
            Some(MotifCommande::FichierTropGros)
        );
    }

    #[test]
    fn un_wav_minimal_est_valide() {
        // Le plus petit en-tete RIFF/WAVE possible, sans aucune donnee derriere : la fonction ne
        // verifie que l'en-tete, pas la presence de son.
        let mut octets = vec![0u8; 44];
        octets[0..4].copy_from_slice(b"RIFF");
        octets[8..12].copy_from_slice(b"WAVE");
        assert!(est_wav_valide(&octets));
    }

    #[test]
    fn un_fichier_sans_en_tete_wav_est_refuse() {
        assert!(!est_wav_valide(
            b"pas du tout un wav, juste du texte avec assez d'octets ici"
        ));
        assert!(!est_wav_valide(&[0u8; 43])); // un octet sous la taille minimale de l'en-tete
        assert!(!est_wav_valide(b""));
    }
}
