// Etape 1 : la fenetre existe et se pilote elle-meme, puisqu'elle n'a pas de barre de titre.
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';

import { brancherReglages } from './reglages';
import { brancherModeles, brancherMoteurs } from './modeles';
import { installerAppairage } from './appairage';
import { installerOnglets } from './onglets';

type Etat = { version: string; plateforme: string; injection: string };

function poser(id: string, valeur: string): void {
  const cible = document.getElementById(id);
  if (cible) cible.textContent = valeur;
}

/// ⛔ Ne jamais ecrire `void promesse`. Un appel au coeur de Tauri peut etre REJETE par le
/// systeme de permissions, et `void` avale ce rejet : le bouton parait inerte, la console reste
/// muette, et rien n'indique qu'il manque une permission dans `capabilities/`.
///
/// C'est exactement ce qui est arrive le 2026-09-17 : `hide()` et `minimize()` etaient rejetes
/// faute de `core:window:allow-hide` et `core:window:allow-minimize`, et il a fallu chercher le
/// defaut ailleurs (zone de glissement, activation de fenetre, coordonnees du clic) avant de le
/// trouver, precisement parce que l'erreur ne se voyait nulle part.
function surErreur(action: string, promesse: Promise<unknown>): void {
  promesse.catch((erreur) => {
    console.error(`Action « ${action} » refusee ou en echec :`, erreur);
  });
}

async function brancherCommandes(): Promise<void> {
  const fenetre = getCurrentWindow();

  document.getElementById('reduire')?.addEventListener('click', () => {
    surErreur('reduire', fenetre.minimize());
  });

  // ⚠️ `hide` et non `close` : Oyant vit dans la zone de notification. Fermer la fenetre doit la
  // masquer, pas arreter la dictee, sinon le raccourci global cesserait de repondre alors que
  // l'icone est toujours la.
  document.getElementById('fermer')?.addEventListener('click', () => {
    surErreur('fermer', fenetre.hide());
  });
}

async function afficherEtat(): Promise<void> {
  try {
    const etat = await invoke<Etat>('etat_plateforme');
    poser('version', etat.version);
    poser('plateforme', etat.plateforme);
    poser('injection', etat.injection);
  } catch (erreur) {
    // Dit plutot que taise : une zone qui reste sur « ... » indefiniment est exactement le
    // silence qui a rendu la version precedente inutilisable.
    poser('version', 'indisponible');
    poser('plateforme', 'indisponible');
    poser('injection', 'indisponible');
    console.error('Etat de la plateforme illisible', erreur);
  }
}

/**
 * Reglage du demarrage automatique.
 *
 * ⛔ Il vit ICI et nulle part ailleurs : le menu de l'icone de la zone de notification ne porte
 * que des actions. Il y a ete jusqu'au 2026-09-17.
 *
 * ⚠️ L'invariant du bloc : **la case affiche ce que le systeme dit, jamais ce qu'on a demande.**
 * On relit donc l'etat apres chaque bascule, y compris quand l'ecriture echoue. Sinon une
 * operation refusee (strategie de groupe, antivirus) laisserait une case cochee qui ment.
 */
async function brancherDemarrageAutomatique(): Promise<void> {
  const case_ = document.getElementById('demarrage') as HTMLInputElement | null;
  const message = document.getElementById('message-reglages');
  if (!case_) return;

  const dire = (texte: string): void => {
    if (message) message.textContent = texte;
  };

  const relire = async (): Promise<void> => {
    case_.checked = await invoke<boolean>('demarrage_automatique');
  };

  try {
    await relire();
    case_.disabled = false;
  } catch (erreur) {
    // On laisse la case inactive : proposer une bascule dont on ne sait pas lire l'etat
    // reviendrait a inventer une valeur de depart.
    dire("L'état du démarrage automatique n'a pas pu être lu.");
    console.error('demarrage_automatique', erreur);
    return;
  }

  case_.addEventListener('change', () => {
    const demande = case_.checked;
    case_.disabled = true;
    dire('');

    void (async () => {
      try {
        case_.checked = await invoke<boolean>('definir_demarrage_automatique', { actif: demande });
      } catch (erreur) {
        dire(String(erreur));
        console.error('definir_demarrage_automatique', erreur);
        // ⚠️ Meme sur echec, on relit : l'operation a pu echouer APRES avoir modifie le systeme.
        await relire().catch(() => {});
      } finally {
        case_.disabled = false;
      }
    })();
  });
}

/**
 * Copyright du pied de page.
 *
 * ⚠️ L'année est **calculée à l'exécution** et jamais écrite en dur : une année figée devient
 * fausse le 1er janvier, et c'est exactement le genre de détail que personne ne pense à corriger.
 * C'est la convention du parc, déjà en place sur les applications mobiles.
 */
function poserCopyright(): void {
  poser('copyright', `© ${new Date().getFullYear()} Breizhzion`);
}

/**
 * Les problèmes rencontrés pendant une dictée.
 *
 * ⛔ Ils remontent par un **événement** et jamais par une boîte de dialogue. Une modale volerait
 * le focus de l'application dans laquelle on est en train d'écrire, ce qui est exactement ce
 * qu'un outil de dictée ne doit jamais faire, et la fenêtre d’Oyant est le plus souvent fermée
 * au moment où le problème survient.
 *
 * ⚠️ Conséquence assumée : si la fenêtre est masquée, le message attend qu'on l'ouvre. L'icône
 * de la zone de notification, elle, est déjà revenue au repos, donc l'absence de texte injecté
 * reste le signal immédiat.
 */
async function brancherProblemesDeDictee(): Promise<void> {
  const { listen } = await import('@tauri-apps/api/event');
  const message = document.getElementById('message-reglages');
  await listen<string>('dictee-probleme', (evenement) => {
    if (message) message.textContent = evenement.payload;
    console.error('dictée', evenement.payload);
  });
}

installerOnglets();
void brancherCommandes();
void afficherEtat();
poserCopyright();
void brancherDemarrageAutomatique();
void brancherProblemesDeDictee();
void brancherReglages();
void brancherMoteurs();
void brancherModeles();

// Appairage. L'ecouteur d'evenement est pose ici et pas a l'affichage de la section : une demande
// arrive quand le telephone la lance, pas quand l'utilisateur regarde le bon endroit.
const zoneAppairage = document.getElementById('appairage');
if (zoneAppairage) installerAppairage(zoneAppairage);
