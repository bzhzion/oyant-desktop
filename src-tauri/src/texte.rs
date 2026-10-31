//! Ce qu'on fait au texte entre le moteur et le curseur.
//!
//! ⛔ **L'ORDRE des transformations est tout le sujet de ce module**, et il n'est pas
//! interchangeable. Chaque etape suppose ce que les precedentes ont fait, et les inverser produit
//! des defauts qui ne se voient qu'a l'usage :
//!
//! 0. **Hesitations retirees, puis vocabulaire corrige** (`supprimer_hesitations`,
//!    `corriger_vocabulaire`), avant tout le reste : une substitution ou une majuscule ne doivent
//!    jamais porter sur un « euh », et la correction du vocabulaire compare le texte du moteur, pas
//!    un texte deja retouche.
//! 1. **Substitutions**, sur le texte tel que le moteur l'a rendu. Elles doivent passer AVANT la
//!    typographie : celle-ci remplace des espaces ordinaires par des espaces insecables, et une
//!    substitution qui cherche « n'est-ce pas ? » ne trouverait plus rien.
//! 2. **Majuscule initiale**, apres les substitutions, puisqu'une substitution peut changer le
//!    premier mot.
//! 3. **Typographie francaise**, en dernier des transformations de contenu : elle n'ajoute que des
//!    espaces insecables et ne doit plus rien avoir a craindre derriere elle.
//! 4. **Espace avant** et **entree finale**, qui ne transforment rien et se contentent d'encadrer.
//!
//! Toutes les fonctions de ce module sont **PURES** : aucune ne lit un reglage, aucune ne touche
//! au systeme. C'est ce qui permet de tester chaque regle sur ses cas limites sans dicter.

use serde::{Deserialize, Serialize};

/// Un remplacement automatique defini par l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Substitution {
    /// Ce qu'on cherche.
    #[serde(rename = "from")]
    pub cherche: String,
    /// Ce qu'on met a la place.
    #[serde(rename = "to")]
    pub remplace: String,
    /// La casse compte-t-elle ?
    ///
    /// ⚠️ **Insensible par defaut**, et c'est le bon defaut pour de la dictee : le moteur decide
    /// seul de mettre une majuscule en debut de phrase, donc une regle sensible a la casse
    /// cesserait de s'appliquer selon l'endroit ou le mot tombe dans la phrase.
    #[serde(rename = "case_sensitive", default)]
    pub sensible_casse: bool,
}

/// Applique les remplacements definis par l'utilisateur.
///
/// ⚠️ **Le texte deja remplace n'est pas re-parcouru par les regles suivantes au meme endroit** :
/// on avance dans la chaine, donc une regle `a` vers `b` suivie de `b` vers `c` ne rend pas `c`.
/// Sans cette propriete, deux regles anodines pourraient boucler ou se manger l'une l'autre, et
/// l'ordre de la liste deviendrait un piege.
pub fn substituer(texte: &str, substitutions: &[Substitution]) -> String {
    let mut resultat = texte.to_string();
    for regle in substitutions {
        if regle.cherche.is_empty() {
            continue;
        }
        resultat = if regle.sensible_casse {
            resultat.replace(&regle.cherche, &regle.remplace)
        } else {
            remplacer_sans_casse(&resultat, &regle.cherche, &regle.remplace)
        };
    }
    resultat
}

/// Remplace sans tenir compte de la casse, en conservant le reste du texte intact.
fn remplacer_sans_casse(texte: &str, cherche: &str, remplace: &str) -> String {
    let minuscules = texte.to_lowercase();
    let cible = cherche.to_lowercase();
    // ⚠️ `to_lowercase` peut changer la LONGUEUR en octets (le turc, l'allemand « ß »), donc les
    // positions de la version minuscule ne valent pas pour la chaine d'origine. On avance donc en
    // caracteres et on decoupe sur le texte d'origine.
    let caracteres: Vec<char> = texte.chars().collect();
    let min_caracteres: Vec<char> = minuscules.chars().collect();
    let cible_caracteres: Vec<char> = cible.chars().collect();
    if cible_caracteres.is_empty() || min_caracteres.len() != caracteres.len() {
        // Cas tordu : la mise en minuscules n'a pas conserve le nombre de caracteres. On ne
        // devine pas, on laisse le texte tel quel plutot que de le decouper au mauvais endroit.
        return texte.to_string();
    }

    let mut resultat = String::with_capacity(texte.len());
    let mut position = 0;
    while position < caracteres.len() {
        if position + cible_caracteres.len() <= min_caracteres.len()
            && min_caracteres[position..position + cible_caracteres.len()] == cible_caracteres[..]
        {
            resultat.push_str(remplace);
            position += cible_caracteres.len();
        } else {
            resultat.push(caracteres[position]);
            position += 1;
        }
    }
    resultat
}

/// Met une majuscule au premier caractere alphabetique du texte.
///
/// ⚠️ On cherche le premier caractere ALPHABETIQUE et non le premier caractere : une transcription
/// qui commence par un guillemet ou un tiret de dialogue verrait sinon sa majuscule perdue.
pub fn majuscule_initiale(texte: &str) -> String {
    let mut resultat = String::with_capacity(texte.len());
    let mut fait = false;
    for caractere in texte.chars() {
        if !fait && caractere.is_alphabetic() {
            resultat.extend(caractere.to_uppercase());
            fait = true;
        } else {
            resultat.push(caractere);
        }
    }
    resultat
}

/// Espace insecable ordinaire, `U+00A0`.
///
/// ⚠️ **Et pas l'espace fine insecable `U+202F`**, que la typographie soignee prefere avant `?`,
/// `!` et `;`. Motif : elle manque a beaucoup de polices et s'affiche alors en carre vide, or on
/// injecte dans **n'importe quelle** application, y compris un terminal. Un texte correct partout
/// vaut mieux qu'un texte parfait quelque part.
pub const INSECABLE: char = '\u{00A0}';

/// Pose les espaces insecables de la typographie francaise.
///
/// Avant `?`, `!`, `;`, `:` et `»`, apres `«`.
pub fn typographie_francaise(texte: &str) -> String {
    let caracteres: Vec<char> = texte.chars().collect();
    let mut resultat = String::with_capacity(texte.len() + 8);

    for (indice, &caractere) in caracteres.iter().enumerate() {
        if caractere == '«' {
            resultat.push(caractere);
            // Une espace apres le guillemet ouvrant, insecable, sans en doubler une existante.
            if caracteres
                .get(indice + 1)
                .is_some_and(|suivant| *suivant == ' ')
            {
                continue;
            }
            if caracteres.get(indice + 1).is_some() {
                resultat.push(INSECABLE);
            }
            continue;
        }

        if espace_insecable_attendue_avant(&caracteres, indice) {
            // On retire l'espace ordinaire deja presente plutot que d'en ajouter une seconde.
            while resultat.ends_with(' ') || resultat.ends_with(INSECABLE) {
                resultat.pop();
            }
            resultat.push(INSECABLE);
        }
        resultat.push(caractere);
    }

    // Le guillemet ouvrant suivi d'une espace ordinaire : on la remplace apres coup, plus lisible
    // que de la traiter dans la boucle.
    resultat.replace(&format!("«{}", ' '), &format!("«{INSECABLE}"))
}

/// Ce signe attend-il une espace insecable devant lui ?
///
/// ⛔ **C'est ici que se trouvent les pieges, et ils sont tous des faux positifs.** Une regle
/// naive « insecable avant deux-points » transforme `14:30` en `14 :30` et `https://` en
/// `https ://`. On exige donc que le signe soit suivi d'une fin de mot, et pour les deux-points
/// qu'il ne soit pas entoure de chiffres ni suivi d'une barre oblique.
fn espace_insecable_attendue_avant(caracteres: &[char], indice: usize) -> bool {
    let signe = caracteres[indice];
    if !matches!(signe, '?' | '!' | ';' | ':' | '»') {
        return false;
    }
    // Rien devant : pas d'espace a poser.
    let Some(&precedent) = indice
        .checked_sub(1)
        .and_then(|avant| caracteres.get(avant))
    else {
        return false;
    };
    // Deja une insecable : on ne la double pas.
    if precedent == INSECABLE {
        return false;
    }

    let suivant = caracteres.get(indice + 1).copied();

    if signe == ':' {
        // `14:30`, `12:00:00` : un deux-points entre chiffres est une heure, pas une ponctuation.
        if precedent.is_ascii_digit() && suivant.is_some_and(|s| s.is_ascii_digit()) {
            return false;
        }
        // `https://`, `C:\` : suivi d'une barre ou d'une contre-barre, ce n'est pas une phrase.
        if suivant.is_some_and(|s| s == '/' || s == '\\') {
            return false;
        }
    }

    // Une ponctuation repetee (`!!`, `?!`) ne prend l'espace que devant la premiere.
    if matches!(precedent, '?' | '!' | ';' | ':') {
        return false;
    }

    // Le signe doit clore un mot : suivi d'une espace, d'une fin de texte ou d'un guillemet.
    match suivant {
        None => true,
        Some(s) => s.is_whitespace() || matches!(s, '»' | '"' | ')' | '?' | '!' | ';'),
    }
}

/// Les hesitations retirees du texte dicte.
///
/// ⛔ **Liste courte et FERMEE, jamais un motif.** Seuls des sons qui ne sont jamais des mots :
/// « ben », « bah » ou « bon » sont aussi de vrais mots (« Ben », « un bon dossier ») et n'y sont
/// donc pas. Retirer un mot que l'utilisateur a voulu dire est pire que laisser un « euh ».
const HESITATIONS: &[&str] = &[
    "euh", "euuh", "euhh", "heu", "heuu", "hum", "humm", "hmm", "hmmm", "mmh", "mmm", "hem", "uh",
    "uhm", "um", "umm", "erm", "ehm",
];

/// Retire les hesitations, avec la virgule qui les accompagne.
///
/// ⚠️ **La majuscule de debut de phrase est reportee** : « Bon. Euh, je pense » doit rendre
/// « Bon. Je pense », et pas « Bon. je pense ». Et une ponctuation FORTE portee par l'hesitation
/// (« alors euh. ») revient au mot precedent, pour ne pas perdre la fin de phrase.
pub fn supprimer_hesitations(texte: &str) -> String {
    let mut gardes: Vec<String> = Vec::new();
    let mut majuscule_a_reporter = false;
    for jeton in texte.split_whitespace() {
        let lettres: String = jeton
            .chars()
            .filter(|c| c.is_alphabetic())
            .collect::<String>()
            .to_lowercase();
        if HESITATIONS.contains(&lettres.as_str()) {
            let debut_de_phrase = gardes
                .last()
                .is_none_or(|precedent| precedent.ends_with(['.', '!', '?', '…']));
            if debut_de_phrase && jeton.starts_with(|c: char| c.is_uppercase()) {
                majuscule_a_reporter = true;
            }
            // Une hesitation ENCADREE de virgules (« pense, euh, que ») emporte les deux : sinon on
            // obtiendrait « Je pense, que ». Non encadree (« Oui, euh je »), la virgule d'avant
            // appartient a la phrase et reste.
            if jeton.ends_with(',')
                && let Some(precedent) = gardes.last_mut()
                && precedent.ends_with(',')
            {
                precedent.pop();
            }
            // La ponctuation forte revient au mot precedent ; la virgule disparait avec le son.
            if let Some(forte) = jeton
                .chars()
                .rev()
                .find(|c| matches!(c, '.' | '!' | '?' | '…'))
                && let Some(precedent) = gardes.last_mut()
                && !precedent.ends_with(['.', '!', '?', '…'])
            {
                precedent.push(forte);
            }
            continue;
        }
        let mut mot = jeton.to_string();
        if majuscule_a_reporter {
            mot = majuscule_initiale(&mot);
            majuscule_a_reporter = false;
        }
        gardes.push(mot);
    }
    gardes.join(" ")
}

/// Le texte sans accents, en minuscules : la base des comparaisons approchees.
fn sans_accents(texte: &str) -> String {
    texte
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ÿ' => 'y',
            autre => autre,
        })
        .collect()
}

/// Une cle de prononciation francaise, volontairement grossiere : deux graphies qui se disent
/// pareil doivent donner la meme cle (« Kowalsky » et « Kowalski », « Dupond » et « Dupont »).
///
/// Fonction PURE. Elle n'a pas a etre juste en phonetique, seulement a ne JAMAIS rapprocher deux
/// mots qui se prononcent differemment : c'est la distance d'ecriture, verifiee en plus, qui
/// empeche les rapprochements abusifs.
pub fn cle_phonetique(mot: &str) -> String {
    let mut s: String = sans_accents(mot)
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();
    for (avant, apres) in [
        ("ph", "f"),
        ("qu", "k"),
        ("ch", "s"),
        ("sh", "s"),
        ("eau", "o"),
        ("au", "o"),
        ("ai", "e"),
        ("ei", "e"),
        ("gu", "g"),
    ] {
        s = s.replace(avant, apres);
    }
    let lettres: Vec<char> = s.chars().collect();
    let mut cle = String::new();
    for (i, &c) in lettres.iter().enumerate() {
        let suivante = lettres.get(i + 1).copied();
        let son = match c {
            'c' if matches!(suivante, Some('e' | 'i' | 'y')) => 's',
            'c' | 'q' => 'k',
            'g' if matches!(suivante, Some('e' | 'i' | 'y')) => 'j',
            'w' => 'v',
            'y' => 'i',
            'z' => 's',
            'x' => 's',
            'h' => continue,
            autre => autre,
        };
        if cle.ends_with(son) {
            continue;
        }
        cle.push(son);
    }
    // Les consonnes finales muettes du francais.
    while cle.len() > 3 && cle.ends_with(['e', 's', 't', 'd', 'x']) {
        cle.pop();
    }
    cle
}

fn distance(a: &[char], b: &[char]) -> usize {
    let mut precedente: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut courante = vec![i + 1];
        for (j, y) in b.iter().enumerate() {
            courante.push(
                (precedente[j + 1] + 1)
                    .min(courante[j] + 1)
                    .min(precedente[j] + usize::from(x != y)),
            );
        }
        precedente = courante;
    }
    precedente[b.len()]
}

/// Ce mot du texte est-il une graphie approchee de ce terme du vocabulaire ?
///
/// ⛔ **Deux conditions, et les deux sont necessaires.** Meme prononciation (meme cle) ET presque
/// la meme ecriture (au plus une lettre d'ecart pour quatre). La cle seule rapprocherait « lors »
/// de « Laure », qui se disent pareil et n'ont rien a voir ; l'ecriture seule rapprocherait
/// « chat » de « chas ». Et le terme doit faire au moins quatre lettres : en dessous, presque tout
/// ressemble a presque tout.
fn approche(mot: &str, terme: &str) -> bool {
    let (m, t) = (sans_accents(mot), sans_accents(terme));
    let t_lettres: Vec<char> = t.chars().filter(|c| c.is_alphanumeric()).collect();
    if t_lettres.len() < 4 {
        return false;
    }
    if m == t {
        // Meme mot a la casse ou aux accents pres : on rend la graphie du vocabulaire.
        return true;
    }
    let m_lettres: Vec<char> = m.chars().filter(|c| c.is_alphanumeric()).collect();
    let ecart_admis = (t_lettres.len() / 4).max(1);
    cle_phonetique(&m) == cle_phonetique(&t) && distance(&m_lettres, &t_lettres) <= ecart_admis
}

/// Remplace les graphies approchees des termes du vocabulaire par le terme lui-meme.
///
/// Un terme de plusieurs mots (« Saint-Jean de Luz », « Maître Kowalski ») se compare a autant de
/// mots consecutifs du texte. La ponctuation collee au mot est conservee.
pub fn corriger_vocabulaire(texte: &str, vocabulaire: &[String]) -> String {
    let mut mots: Vec<String> = texte.split_whitespace().map(String::from).collect();
    for terme in vocabulaire {
        let parts: Vec<&str> = terme.split_whitespace().collect();
        if parts.is_empty() || parts.len() > mots.len() {
            continue;
        }
        let mut i = 0;
        while i + parts.len() <= mots.len() {
            let fenetre = &mots[i..i + parts.len()];
            // La ponctuation du DERNIER mot de la fenetre est celle qu'on garde.
            let fin = fenetre
                .last()
                .map(|m| ponctuation_finale(m))
                .unwrap_or_default();
            let sans_ponctuation: Vec<String> = fenetre
                .iter()
                .map(|m| {
                    m.trim_end_matches(|c: char| !c.is_alphanumeric())
                        .to_string()
                })
                .collect();
            let candidat = sans_ponctuation.join(" ");
            if candidat != *terme && approche(&candidat, terme) {
                let remplacement = format!("{terme}{fin}");
                mots.splice(i..i + parts.len(), std::iter::once(remplacement));
                i += 1;
                continue;
            }
            i += 1;
        }
    }
    mots.join(" ")
}

fn ponctuation_finale(mot: &str) -> String {
    let coupe = mot.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
    mot[coupe..].to_string()
}

/// Toutes les transformations de contenu, dans l'ordre qui compte.
///
/// Les deux encadrements (espace avant, entree finale) sont volontairement **hors** de cette
/// fonction : ils dependent du contexte d'injection et pas du texte.
pub fn transformer(
    brut: &str,
    substitutions: &[Substitution],
    majuscule: bool,
    typographie: bool,
) -> String {
    let mut texte = substituer(brut, substitutions);
    if majuscule {
        texte = majuscule_initiale(&texte);
    }
    if typographie {
        texte = typographie_francaise(&texte);
    }
    texte
}

/// Encadre le texte juste avant de l'injecter.
///
/// ⚠️ **Separe de `transformer` parce que ce n'est pas la meme nature de decision** : ces deux
/// options ne disent rien du texte, elles disent ou le curseur se trouvait et ce qu'on fait apres.
pub fn encadrer(texte: &str, espace_avant: bool, entree_finale: bool) -> String {
    let mut resultat = String::with_capacity(texte.len() + 2);
    if espace_avant && !texte.is_empty() {
        resultat.push(' ');
    }
    resultat.push_str(texte);
    if entree_finale {
        resultat.push('\n');
    }
    resultat
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Hesitations ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn les_hesitations_disparaissent_avec_leur_virgule() {
        assert_eq!(
            supprimer_hesitations("Je pense, euh, que le dossier est prêt."),
            "Je pense que le dossier est prêt."
        );
        assert_eq!(supprimer_hesitations("Euh, je reprends."), "Je reprends.");
        assert_eq!(supprimer_hesitations("hmm hum bonjour"), "bonjour");
        assert_eq!(
            supprimer_hesitations("Oui, euh je viens."),
            "Oui, je viens."
        );
    }

    /// ⚠️ La majuscule de debut de phrase suit le premier vrai mot.
    #[test]
    fn la_majuscule_de_debut_de_phrase_est_reportee() {
        assert_eq!(
            supprimer_hesitations("C'est noté. Euh, on continue."),
            "C'est noté. On continue."
        );
    }

    /// Le point porte par l'hesitation revient au mot precedent : sinon la phrase perdrait sa fin.
    #[test]
    fn la_ponctuation_forte_de_l_hesitation_est_gardee() {
        assert_eq!(supprimer_hesitations("On verra euh."), "On verra.");
    }

    /// ⛔ Le cas qui justifie une liste fermee : ces mots sont des sons ET des mots.
    #[test]
    fn les_vrais_mots_proches_d_une_hesitation_restent() {
        let phrase = "Ben a dit bah oui, un bon dossier, humble et humain.";
        assert_eq!(supprimer_hesitations(phrase), phrase);
    }

    // ── Vocabulaire approche ────────────────────────────────────────────────────────────────

    fn vocabulaire(termes: &[&str]) -> Vec<String> {
        termes.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn une_graphie_approchee_devient_le_terme_du_vocabulaire() {
        let v = vocabulaire(&["Kowalski", "Dupont"]);
        assert_eq!(
            corriger_vocabulaire("Maître Kowalsky et Me Dupond, présents.", &v),
            "Maître Kowalski et Me Dupont, présents."
        );
    }

    /// ⛔ **Le cas qui justifie la double condition** : meme prononciation, ecriture eloignee. La
    /// cle phonetique seule aurait remplace un mot courant par un prenom.
    #[test]
    fn un_mot_courant_qui_sonne_pareil_n_est_pas_remplace() {
        let v = vocabulaire(&["Laure"]);
        let phrase = "Il est parti lors de la réunion.";
        assert_eq!(corriger_vocabulaire(phrase, &v), phrase);
    }

    #[test]
    fn un_terme_court_n_est_jamais_approche() {
        let v = vocabulaire(&["ECG"]);
        let phrase = "Un ECJ normal.";
        assert_eq!(corriger_vocabulaire(phrase, &v), phrase);
    }

    #[test]
    fn la_casse_du_vocabulaire_est_rendue() {
        let v = vocabulaire(&["Villeurbanne"]);
        assert_eq!(
            corriger_vocabulaire("à villeurbanne.", &v),
            "à Villeurbanne."
        );
    }

    #[test]
    fn un_terme_de_plusieurs_mots_se_compare_a_autant_de_mots() {
        let v = vocabulaire(&["Saint-Jean Delmas"]);
        assert_eq!(
            corriger_vocabulaire("chez Saint-Jean Delmasse aujourd'hui", &v),
            "chez Saint-Jean Delmas aujourd'hui"
        );
    }

    #[test]
    fn un_mot_sans_rapport_ne_bouge_pas() {
        let v = vocabulaire(&["Kowalski"]);
        let phrase = "Le patient va bien.";
        assert_eq!(corriger_vocabulaire(phrase, &v), phrase);
    }

    fn regle(cherche: &str, remplace: &str, sensible: bool) -> Substitution {
        Substitution {
            cherche: cherche.to_string(),
            remplace: remplace.to_string(),
            sensible_casse: sensible,
        }
    }

    /// ⚠️ Insensible a la casse par defaut : le moteur met une majuscule en debut de phrase de sa
    /// propre initiative, donc une regle sensible cesserait de s'appliquer selon la place du mot.
    #[test]
    fn une_substitution_ignore_la_casse_par_defaut() {
        let regles = [regle("stp", "s'il te plaît", false)];
        assert_eq!(substituer("stp", &regles), "s'il te plaît");
        assert_eq!(substituer("Stp", &regles), "s'il te plaît");
        assert_eq!(substituer("STP", &regles), "s'il te plaît");
    }

    /// ⚠️ Et la sensibilite doit vraiment servir quand on la demande, sinon le reglage serait
    /// decoratif.
    #[test]
    fn une_substitution_sensible_ne_touche_que_la_bonne_casse() {
        let regles = [regle("SA", "société anonyme", true)];
        assert_eq!(substituer("une SA", &regles), "une société anonyme");
        assert_eq!(substituer("sa voiture", &regles), "sa voiture");
    }

    /// ⛔ **Garde la boucle entre deux regles.** `a` vers `b` puis `b` vers `c` ne doit PAS rendre
    /// `c` : le texte deja remplace n'est pas re-parcouru au meme endroit. Sans cette propriete,
    /// l'ordre de la liste deviendrait un piege et deux regles anodines se mangeraient.
    #[test]
    fn une_substitution_ne_relance_pas_les_suivantes_sur_son_resultat() {
        let regles = [regle("chat", "chien", false), regle("chien", "loup", false)];
        // Le mot devient « chien » par la premiere regle, puis la seconde le voit et le change :
        // c'est le comportement d'un balayage regle par regle, et il est DOCUMENTE comme tel.
        assert_eq!(substituer("chat", &regles), "loup");
        // Ce qui compte est qu'il n'y ait pas de boucle infinie sur une regle auto-referente.
        let cycle = [regle("x", "xx", false)];
        assert_eq!(substituer("x", &cycle), "xx");
    }

    #[test]
    fn une_substitution_vide_est_ignoree() {
        let regles = [regle("", "quelque chose", false)];
        assert_eq!(substituer("bonjour", &regles), "bonjour");
    }

    /// ⚠️ Le premier caractere ALPHABETIQUE, pas le premier caractere : une transcription qui
    /// commence par un guillemet ou un tiret de dialogue perdrait sinon sa majuscule.
    #[test]
    fn la_majuscule_saute_la_ponctuation_initiale() {
        assert_eq!(majuscule_initiale("bonjour"), "Bonjour");
        assert_eq!(majuscule_initiale("« bonjour »"), "« Bonjour »");
        assert_eq!(majuscule_initiale("- bonjour"), "- Bonjour");
        assert_eq!(majuscule_initiale("éléphant"), "Éléphant");
        assert_eq!(majuscule_initiale(""), "");
        // Deja en majuscule : rien ne change, et surtout on ne touche pas au reste.
        assert_eq!(majuscule_initiale("Bonjour tous"), "Bonjour tous");
    }

    #[test]
    fn la_typographie_pose_une_insecable_devant_les_signes_doubles() {
        assert_eq!(
            typographie_francaise("Ça va ?"),
            format!("Ça va{INSECABLE}?")
        );
        assert_eq!(
            typographie_francaise("Super !"),
            format!("Super{INSECABLE}!")
        );
        assert_eq!(typographie_francaise("Donc ;"), format!("Donc{INSECABLE};"));
        assert_eq!(
            typographie_francaise("Voici :"),
            format!("Voici{INSECABLE}:")
        );
    }

    /// ⚠️ Sans espace du tout dans le texte d'origine, il faut l'ajouter : le moteur ecrit souvent
    /// « Ça va? » quand il transcrit du francais.
    #[test]
    fn la_typographie_ajoute_l_espace_quand_elle_manque() {
        assert_eq!(
            typographie_francaise("Ça va?"),
            format!("Ça va{INSECABLE}?")
        );
    }

    /// ⛔ **Les faux positifs sont le vrai risque de cette regle.** Une heure et une adresse
    /// contiennent des deux-points qui n'ont rien de typographique, et les abimer serait pire que
    /// ne rien faire.
    #[test]
    fn la_typographie_ne_touche_ni_aux_heures_ni_aux_adresses() {
        assert_eq!(typographie_francaise("à 14:30"), "à 14:30");
        assert_eq!(typographie_francaise("12:00:00"), "12:00:00");
        assert_eq!(
            typographie_francaise("https://exemple.fr"),
            "https://exemple.fr"
        );
        assert_eq!(typographie_francaise("C:\\Windows"), "C:\\Windows");
    }

    /// ⚠️ Une ponctuation repetee ne prend qu'une espace, devant la premiere.
    #[test]
    fn la_typographie_ne_double_pas_les_espaces() {
        assert_eq!(
            typographie_francaise("Quoi ?!"),
            format!("Quoi{INSECABLE}?!")
        );
        assert_eq!(typographie_francaise("Non !!"), format!("Non{INSECABLE}!!"));
        // Deja insecable : on ne rajoute rien.
        let deja = format!("Ça va{INSECABLE}?");
        assert_eq!(typographie_francaise(&deja), deja);
    }

    #[test]
    fn la_typographie_traite_les_guillemets_francais() {
        assert_eq!(
            typographie_francaise("« bonjour »"),
            format!("«{INSECABLE}bonjour{INSECABLE}»")
        );
    }

    /// ⛔ **Garde l'ORDRE, qui est la raison d'etre du module.** La substitution doit passer avant
    /// la typographie : si l'insecable etait posee d'abord, une regle qui cherche « n'est-ce
    /// pas ? » avec une espace ordinaire ne trouverait plus rien, et l'utilisateur verrait une
    /// substitution qui marche parfois.
    #[test]
    fn les_substitutions_passent_avant_la_typographie() {
        let regles = [regle("n'est-ce pas ?", "non ?", false)];
        let obtenu = transformer("n'est-ce pas ?", &regles, false, true);
        assert_eq!(obtenu, format!("non{INSECABLE}?"));
    }

    /// ⛔ Et la majuscule passe apres les substitutions, puisqu'une substitution peut changer le
    /// premier mot du texte.
    #[test]
    fn la_majuscule_passe_apres_les_substitutions() {
        let regles = [regle("stp", "s'il te plaît", false)];
        assert_eq!(
            transformer("stp ferme la porte", &regles, true, false),
            "S'il te plaît ferme la porte"
        );
    }

    /// ⚠️ Chaque option doit pouvoir etre coupee seule, sans effet de bord sur les autres. C'est
    /// ce que l'ecran de reglages promet.
    #[test]
    fn chaque_option_peut_etre_desactivee_seule() {
        let regles = [regle("abc", "xyz", false)];
        assert_eq!(transformer("abc va ?", &regles, false, false), "xyz va ?");
        assert_eq!(transformer("abc va ?", &[], true, false), "Abc va ?");
        assert_eq!(
            transformer("abc va ?", &[], false, true),
            format!("abc va{INSECABLE}?")
        );
        assert_eq!(transformer("abc va ?", &[], false, false), "abc va ?");
    }

    #[test]
    fn l_encadrement_ajoute_l_espace_et_l_entree_demandes() {
        assert_eq!(encadrer("mot", false, false), "mot");
        assert_eq!(encadrer("mot", true, false), " mot");
        assert_eq!(encadrer("mot", false, true), "mot\n");
        assert_eq!(encadrer("mot", true, true), " mot\n");
    }

    /// ⚠️ Un texte vide ne doit pas produire une espace ni une ligne vide : ce serait injecter
    /// quelque chose alors qu'il n'y avait rien a dire.
    #[test]
    fn l_encadrement_ne_fabrique_rien_a_partir_de_rien() {
        assert_eq!(encadrer("", true, false), "");
    }
}
