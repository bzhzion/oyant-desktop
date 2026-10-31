//! Ramène la fenêtre principale dans la zone utile de l'écran au démarrage.
//!
//! ⛔ La fenêtre fait 640 px LOGIQUES de haut. À 175 % sur un écran 1080p, la zone utile au-dessus
//! de la barre des tâches n'en offre qu'environ 570 : centrée, la fenêtre débordait en haut, et
//! comme elle n'a pas de barre de titre Windows, sa zone de déplacement et sa croix sortaient de
//! l'écran sans moyen de les rattraper. Relevé le 2026-10-31 en vérifiant les échelles élevées.
//!
//! Tout se calcule en pixels PHYSIQUES, comme la zone utile que rend le système : mélanger
//! logiques et physiques est précisément l'erreur qui ne se voit qu'à 150 % et au-delà.

use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};

/// Un rectangle en pixels physiques : origine, puis largeur et hauteur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub largeur: u32,
    pub hauteur: u32,
}

/// Où mettre la fenêtre pour qu'elle tienne dans la zone utile, ou `None` si elle y tient déjà.
///
/// Réduite à la zone si elle la dépasse, sans descendre sous son minimum (mieux vaut un bas de
/// fenêtre coupé qu'un HAUT coupé : c'est en haut que sont la poignée et la croix), puis centrée.
pub fn ajuster(fenetre: Rect, zone: Rect, minimum: (u32, u32)) -> Option<Rect> {
    let dedans = fenetre.x >= zone.x
        && fenetre.y >= zone.y
        && fenetre.x + fenetre.largeur as i32 <= zone.x + zone.largeur as i32
        && fenetre.y + fenetre.hauteur as i32 <= zone.y + zone.hauteur as i32;
    if dedans {
        return None;
    }
    let largeur = fenetre.largeur.min(zone.largeur).max(minimum.0);
    let hauteur = fenetre.hauteur.min(zone.hauteur).max(minimum.1);
    // ⚠️ Le haut ne remonte jamais au-dessus de la zone, même quand la fenêtre minimale est plus
    // haute qu'elle : `saturating_sub` arrête l'écart à zéro, donc la poignée reste visible.
    let x = zone.x + (zone.largeur.saturating_sub(largeur) / 2) as i32;
    let y = zone.y + (zone.hauteur.saturating_sub(hauteur) / 2) as i32;
    Some(Rect { x, y, largeur, hauteur })
}

/// Applique `ajuster` à la fenêtre, sur l'écran où elle s'ouvre.
pub fn ramener(fenetre: &WebviewWindow, minimum_logique: (f64, f64)) {
    let Ok(Some(ecran)) = fenetre.current_monitor() else {
        return;
    };
    let (Ok(position), Ok(taille)) = (fenetre.outer_position(), fenetre.outer_size()) else {
        return;
    };
    let zone = ecran.work_area();
    let echelle = ecran.scale_factor();
    let minimum = (
        (minimum_logique.0 * echelle) as u32,
        (minimum_logique.1 * echelle) as u32,
    );
    let actuelle = Rect {
        x: position.x,
        y: position.y,
        largeur: taille.width,
        hauteur: taille.height,
    };
    let utile = Rect {
        x: zone.position.x,
        y: zone.position.y,
        largeur: zone.size.width,
        hauteur: zone.size.height,
    };
    if let Some(cible) = ajuster(actuelle, utile, minimum) {
        let _ = fenetre.set_size(PhysicalSize::new(cible.largeur, cible.hauteur));
        let _ = fenetre.set_position(PhysicalPosition::new(cible.x, cible.y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Écran 1920 x 1080 à 175 %, barre des tâches de 48 px logiques (84 physiques).
    const ZONE_175: Rect = Rect { x: 0, y: 0, largeur: 1920, hauteur: 996 };
    const MINIMUM_175: (u32, u32) = (805, 735);

    #[test]
    fn une_fenetre_qui_tient_ne_bouge_pas() {
        let fenetre = Rect { x: 100, y: 100, largeur: 980, hauteur: 800 };
        assert_eq!(ajuster(fenetre, ZONE_175, MINIMUM_175), None);
    }

    #[test]
    fn a_175_pour_cent_la_fenetre_est_reduite_et_son_haut_reste_visible() {
        // 560 x 640 logiques à 175 % = 980 x 1120, centrée par le système : haut à -62.
        let fenetre = Rect { x: 470, y: -62, largeur: 980, hauteur: 1120 };
        let cible = ajuster(fenetre, ZONE_175, MINIMUM_175).expect("doit être ramenée");
        assert_eq!(cible.hauteur, 996);
        assert_eq!(cible.y, 0);
        assert_eq!(cible.largeur, 980);
    }

    #[test]
    fn le_haut_ne_sort_jamais_meme_si_le_minimum_depasse_la_zone() {
        let zone = Rect { x: 0, y: 0, largeur: 900, hauteur: 600 };
        let fenetre = Rect { x: 0, y: -100, largeur: 900, hauteur: 900 };
        let cible = ajuster(fenetre, zone, (800, 700)).expect("doit être ramenée");
        assert_eq!(cible.hauteur, 700);
        assert!(cible.y >= zone.y, "le haut de la fenêtre est sorti de l'écran");
    }

    #[test]
    fn un_second_ecran_garde_son_origine() {
        // Écran secondaire à gauche du principal, origine négative.
        let zone = Rect { x: -1920, y: 0, largeur: 1920, hauteur: 996 };
        let fenetre = Rect { x: -1450, y: -62, largeur: 980, hauteur: 1120 };
        let cible = ajuster(fenetre, zone, MINIMUM_175).expect("doit être ramenée");
        assert_eq!(cible.x, -1920 + 470);
        assert_eq!(cible.y, 0);
    }
}
