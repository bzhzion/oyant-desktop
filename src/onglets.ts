// Barre d'onglets du panneau de reglages.
//
// ⚠️ Motif WAI-ARIA « tabs », activation automatique : deplacer le focus avec les fleches
// CHANGE immediatement l'onglet actif, sans attendre Entree. C'est le motif recommande quand
// afficher le panneau ne coute rien (un `hidden` pose/retire), ce qui est notre cas : aucune
// requete, aucun calcul, juste des blocs deja construits qu'on montre ou cache.
//
// ⛔ **Tabindex en carrousel (roving tabindex)**, pas un `tabindex="0"` sur chaque bouton : un
// seul onglet est atteignable par Tab a la fois (celui qui est actif), les fleches deplacent le
// focus ENTRE les onglets. Mettre `tabindex="0"` partout ferait que Tab doive traverser les six
// boutons un par un avant d'atteindre le contenu, ce qu'aucun lecteur d'ecran n'attend d'une
// barre d'onglets.

/** Bascule l'affichage et l'etat ARIA, sans aucune transition : voir `docs/direction-visuelle.md`,
 * section Mouvement : rien ne bouge en dehors de l'ouverture de la fenetre et du temoin. */
function activer(onglets: HTMLButtonElement[], panneaux: HTMLElement[], index: number): void {
  for (let i = 0; i < onglets.length; i += 1) {
    const actif = i === index;
    onglets[i].setAttribute('aria-selected', String(actif));
    onglets[i].tabIndex = actif ? 0 : -1;
    panneaux[i].toggleAttribute('hidden', !actif);
  }

  // ⛔ **Les six panneaux partagent UN SEUL conteneur defilant** (`.contenu-onglets`), et pas un
  // defilement propre a chacun. Sans cette remise a zero, un onglet long defile a moitie, puis
  // un onglet plus court choisi ensuite semble revenir en haut par PUR HASARD : c'est le
  // navigateur qui borne lui-meme le defilement a ce que le nouveau contenu permet, et rien ne le
  // garantit des que le nouvel onglet est au moins aussi long que la position quittee.
  document.querySelector('.contenu-onglets')?.scrollTo(0, 0);
}

export function installerOnglets(): void {
  const barre = document.querySelector<HTMLElement>('.onglets');
  if (!barre) return;

  const onglets = Array.from(barre.querySelectorAll<HTMLButtonElement>('.onglet'));
  // ⚠️ Les panneaux sont retrouves par l'ID que CHAQUE onglet designe dans `aria-controls`, et
  // pas par position : une barre et des panneaux qui divergent silencieusement en cas d'oubli
  // d'un attribut est pire qu'une erreur bruyante au chargement.
  const panneaux = onglets.map((onglet) => {
    const id = onglet.getAttribute('aria-controls');
    const panneau = id ? document.getElementById(id) : null;
    if (!panneau) {
      console.error(`Onglet « ${onglet.id} » sans panneau : aria-controls="${id}" introuvable.`);
    }
    return panneau as HTMLElement;
  });
  if (panneaux.some((p) => !p)) return;

  onglets.forEach((onglet, index) => {
    onglet.addEventListener('click', () => activer(onglets, panneaux, index));

    onglet.addEventListener('keydown', (evenement) => {
      let cible = -1;
      if (evenement.key === 'ArrowRight') cible = (index + 1) % onglets.length;
      else if (evenement.key === 'ArrowLeft') cible = (index - 1 + onglets.length) % onglets.length;
      else if (evenement.key === 'Home') cible = 0;
      else if (evenement.key === 'End') cible = onglets.length - 1;
      else return;

      evenement.preventDefault();
      activer(onglets, panneaux, cible);
      onglets[cible].focus();
    });
  });
}
