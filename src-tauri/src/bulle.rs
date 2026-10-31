//! La bulle de transcription en direct : une petite fenetre qui montre le texte pendant qu'on parle.
//!
//! ⛔ **Elle ne doit JAMAIS prendre le focus.** Le texte definitif s'ecrit dans l'application qui a
//! le focus : si la bulle le prenait en s'affichant, la dictee atterrirait dans Oyant au lieu du
//! courrier de l'utilisateur. D'ou trois protections cumulees : la fenetre est non focalisable
//! (`focusable(false)`), les clics la traversent, et sous Windows elle s'affiche par
//! `SW_SHOWNOACTIVATE`, la garantie du systeme qu'un affichage n'active pas la fenetre.
//!
//! ⚠️ **Elle montre un APERCU, elle n'ecrit rien.** Taper le texte provisoire dans l'application
//! puis le corriger demanderait d'effacer dans un logiciel qu'on ne controle pas. Le texte qui
//! compte reste celui de la transcription finale, ecrit quand on relache la touche.
//!
//! La page (`public/bulle.html`) n'a aucun script : c'est ce module qui y ecrit, par `eval`. La
//! fenetre n'a donc besoin d'aucune permission.

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

const ETIQUETTE: &str = "bulle";
const LARGEUR: u32 = 560;
const HAUTEUR: u32 = 92;
/// Marge au-dessus du bas de l'ecran, pour rester au-dessus de la barre des taches.
const MARGE_BAS: u32 = 96;
/// Ce que la bulle garde d'une dictee longue : deux lignes de texte, a peu pres.
const CARACTERES_MAX: usize = 140;

/// La fin d'un texte, coupee a une frontiere de mot et precedee de « … » si elle est tronquee.
///
/// Fonction PURE : c'est ce qu'on vient de dire qui doit rester lisible, pas le debut.
pub fn fin_du_texte(texte: &str) -> String {
    let texte = texte.trim();
    let nombre = texte.chars().count();
    if nombre <= CARACTERES_MAX {
        return texte.to_string();
    }
    let debut = texte
        .char_indices()
        .nth(nombre - CARACTERES_MAX)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let fin = &texte[debut..];
    // On ne commence pas au milieu d'un mot : on saute jusqu'a l'espace suivant.
    let fin = fin.split_once(' ').map(|(_, reste)| reste).unwrap_or(fin);
    format!("… {}", fin.trim_start())
}

fn fenetre(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(existante) = app.get_webview_window(ETIQUETTE) {
        return Some(existante);
    }
    let construite =
        WebviewWindowBuilder::new(app, ETIQUETTE, WebviewUrl::App("bulle.html".into()))
            .title("Oyant : transcription en direct")
            .inner_size(LARGEUR as f64, HAUTEUR as f64)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focusable(false)
            .focused(false)
            .visible(false)
            .build();
    match construite {
        Ok(fenetre) => {
            let _ = fenetre.set_ignore_cursor_events(true);
            Some(fenetre)
        }
        Err(erreur) => {
            eprintln!("bulle : {erreur}");
            None
        }
    }
}

/// Place la bulle en bas et au centre de l'ecran principal.
fn placer(fenetre: &tauri::WebviewWindow) {
    let Ok(Some(ecran)) = fenetre.primary_monitor() else {
        return;
    };
    let taille = ecran.size();
    let origine = ecran.position();
    let echelle = ecran.scale_factor();
    let largeur = (LARGEUR as f64 * echelle) as u32;
    let hauteur = (HAUTEUR as f64 * echelle) as u32;
    let marge = (MARGE_BAS as f64 * echelle) as u32;
    let x = origine.x + (taille.width.saturating_sub(largeur) / 2) as i32;
    let y = origine.y + taille.height.saturating_sub(hauteur + marge) as i32;
    let _ = fenetre.set_size(PhysicalSize::new(largeur, hauteur));
    let _ = fenetre.set_position(PhysicalPosition::new(x, y));
}

#[cfg(windows)]
fn afficher_sans_activer(fenetre: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindow};
    match fenetre.hwnd() {
        Ok(hwnd) => unsafe {
            let _ = ShowWindow(HWND(hwnd.0), SW_SHOWNOACTIVATE);
        },
        Err(erreur) => eprintln!("bulle : {erreur}"),
    }
}

#[cfg(not(windows))]
fn afficher_sans_activer(fenetre: &tauri::WebviewWindow) {
    let _ = fenetre.show();
}

/// Ecrit un texte dans la bulle. Le texte est passe en JSON, jamais concatene dans le script :
/// une dictee qui contiendrait une apostrophe ou une balise ne doit rien pouvoir executer.
pub fn ecrire(app: &AppHandle, texte: &str) {
    let Some(fenetre) = app.get_webview_window(ETIQUETTE) else {
        return;
    };
    let Ok(json) = serde_json::to_string(&fin_du_texte(texte)) else {
        return;
    };
    let _ = fenetre.eval(format!(
        "document.getElementById('texte').textContent = {json};"
    ));
}

/// Affiche la bulle, sans lui donner le focus, avec le message d'attente.
pub fn montrer(app: &AppHandle) {
    let Some(fenetre) = fenetre(app) else {
        return;
    };
    placer(&fenetre);
    ecrire(app, "Je vous écoute…");
    afficher_sans_activer(&fenetre);
}

pub fn cacher(app: &AppHandle) {
    if let Some(fenetre) = app.get_webview_window(ETIQUETTE) {
        let _ = fenetre.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_texte_court_reste_entier() {
        assert_eq!(fin_du_texte("  Bonjour à tous. "), "Bonjour à tous.");
    }

    /// ⚠️ C'est la FIN qui doit rester : on lit ce qu'on vient de dire, pas le debut de la dictee.
    #[test]
    fn un_texte_long_garde_sa_fin_a_une_frontiere_de_mot() {
        let long = "mot ".repeat(60) + "dernière phrase dite";
        let fin = fin_du_texte(&long);
        assert!(fin.starts_with("… "), "{fin}");
        assert!(fin.ends_with("dernière phrase dite"), "{fin}");
        assert!(fin.chars().count() <= CARACTERES_MAX + 2, "{fin}");
        assert!(!fin.contains("… o"), "coupe au milieu d'un mot : {fin}");
    }

    /// Les accents comptent pour un caractere : couper par octets tomberait au milieu d'un « é ».
    #[test]
    fn les_accents_ne_cassent_pas_la_coupe() {
        let long = "été ".repeat(80);
        let fin = fin_du_texte(&long);
        assert!(fin.starts_with("… été"), "{fin}");
    }
}
