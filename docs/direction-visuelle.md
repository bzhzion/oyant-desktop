# Direction visuelle

Posee a l'etape 1, avant toute fonctionnalite, parce que l'ancienne version avait un moteur
correct et une interface qui rendait le tout inutilisable.

## Le parti pris : une face avant d'appareil

Oyant n'est pas une application qu'on parcourt, c'est un **instrument qu'on convoque**. Il vit
dans la zone de notification, on l'ouvre quelques secondes, on regle quelque chose, on le referme.
Une mise en page de tableau de bord, avec sa barre laterale et ses cartes, serait un contresens :
elle invite a explorer, alors qu'ici tout doit etre lisible d'un coup d'oeil.

D'ou le parti pris : **la face avant d'un appareil de mesure audio**. Plaque sombre, libelles
graves, groupes separes par des filets, et **un seul temoin lumineux**. C'est honnete au produit
(de l'audio, du local, de la precision) et ca evite le enieme panneau sombre generique.

## Ce dont on se souvient : le temoin

Le logo est un **anneau et un disque separes par un vide transparent**. On le reprend comme
**unique element vivant** de l'interface, en haut a gauche, et il porte l'etat :

| Etat | Anneau | Libelle |
|---|---|---|
| Repos | `border`, immobile | « Pret » |
| Ecoute | `accent`, respiration lente | « Ecoute » |
| Enregistrement | `danger`, pulsation | « Enregistrement » |
| Traitement | `preparation` (ambre), rotation | « Transcription » |

⚠️ **La couleur ne porte jamais l'etat seule** (WCAG 1.4.1) : le libelle le nomme toujours, et la
nature du mouvement differe (respiration, pulsation, rotation), ce qui reste perceptible sans
distinguer les teintes.

C'est **le meme objet que l'icone de la zone de notification**, dans le meme code couleur. Ouvrir
la fenetre ne change pas de langage : on retrouve le temoin en grand.

## Typographie

Trois roles, deux familles, et un contraste voulu entre l'humain et la machine.

| Role | Fonte | Pourquoi |
|---|---|---|
| Titres | **Fraunces** | Deja l'identite de l'app iOS, donc continuite reelle entre le mobile et le bureau. Son dessin a du caractere la ou une grotesque neutre n'en aurait aucun |
| Texte courant | **IBM Plex Sans** | Lisible a petite taille, sobre, elle laisse les titres parler |
| Valeurs, etats, tailles, libelles de section | **IBM Plex Mono** | C'est elle qui fait la face avant : les libelles graves, en capitales espacees, et tout ce qui est une mesure (« 1,5 Go », « 42 % », « verifie ») |

⚠️ **Les fontes sont EMBARQUEES dans le binaire, jamais chargees depuis un CDN.** Une application
de dictee locale doit s'ouvrir identique sans reseau, et aller chercher une fonte en ligne
contredirait en plus la promesse du produit. Le CDN du parc sert les sites, pas celui-ci.

## Composition

**Un bandeau en tete**, qui est aussi la zone de glissement de la fenetre : le temoin a gauche, le
nom grave a cote, les commandes de fenetre a droite. C'est la plaque de marque de l'appareil.

**En dessous, des modules** separes par des filets d'un pixel en `border`, chacun introduit par un
libelle de section en mono, capitales, interlettrage large, en `textMuted`. Pas de cartes
flottantes ni d'ombres portees : sur une face avant, les groupes sont graves, pas empiles.

**Densite controlee plutot que grand vide.** La fenetre est petite et convoquee pour une tache
precise ; l'air se met entre les groupes, pas a l'interieur.

### Les onglets du panneau de reglages (2026-10-01)

La liste de reglages a grossi groupe apres groupe jusqu'a devenir une seule colonne trop longue
pour une fenetre pensee petite. Six onglets la remplacent : Transcription, Capture, Sortie,
Telephone, Application, A propos. ⚠️ **Ce n'est pas la barre laterale de tableau de bord que le
parti pris refuse plus haut** : les onglets restent HORIZONTAUX, en tete du panneau, dans le meme
mono-capitales-interlettrage que les libelles de section : un selecteur de canal sur une face
avant d'appareil, pas une navigation qui invite a explorer.

Le filet actif **remplace** celui, neutre, du bas de la barre, plutot que de s'y ajouter : deux
filets l'un sous l'autre auraient fait une double ligne, lourde et non voulue.

⚠️ **Le mode « Reglages avances » reste UNIQUE, hors des onglets.** C'est un mode qui s'applique a
toutes les sections a la fois, pas une donnee propre a l'une d'elles : le repeter dans chaque
onglet l'aurait fait diverger (coche ici, pas la).

⛔ **Les six panneaux partagent UN SEUL conteneur defilant.** Changer d'onglet remet ce defilement
a zero explicitement : compter sur le fait que le nouvel onglet est plus court et recadre tout
seul est une coincidence de longueur, pas une garantie, et ca s'est vu au premier essai reel.

## Le fond n'est pas plat

Un degrade vertical tres faible depuis `bg`, plus un **grain a 2 ou 3 pour cent**. Ce n'est pas
decoratif : un degrade sombre **bande** visiblement sur la plupart des ecrans, et le grain est ce
qui casse les bandes. Il donne au passage la matiere de metal peint que le parti pris demande.

## Le gestionnaire de modeles, piece maitresse

C'est le premier ecran utile, donc il donne le ton. **Une ligne par modele** : le nom, la taille en
mono, un etat nomme, et **une barre de progression qui est le filet inferieur de la ligne**.

Au repos le filet est en `border`. Pendant le telechargement il se remplit en `accent`. Pendant la
**verification d'integrite** il passe en `preparation` et le libelle dit « verification ». Une fois
pret, une coche en `accent` et le filet redevient neutre.

⚠️ L'etat est **toujours ecrit**, et la verification d'integrite est **montree** au lieu d'etre
silencieuse : c'est ce qui distingue « le fichier est la » de « le fichier est bon », distinction
que l'app iOS a appris a faire a ses depens.

## Mouvement

Une seule orchestration, a l'ouverture de la fenetre, parce que la fenetre est **convoquee** et
doit donner l'impression de s'eveiller : le bandeau parait, puis les modules avec un decalage de
40 ms, sur 180 ms, en sortie douce. Rien d'autre ne bouge, sauf le temoin et les barres de
progression.

`prefers-reduced-motion` supprime l'orchestration et remplace les animations du temoin par des
changements d'etat francs.

## Fenetre sans barre de titre : les pieges

⚠️ **Une zone de glissement rend ses enfants inertes.** `-webkit-app-region: drag` avale les clics,
donc tout bouton pose dessus doit porter `no-drag` explicitement, sinon il ne repond jamais et rien
ne le signale.

⚠️ **Supprimer la barre de titre supprime aussi ce qu'elle offrait gratuitement** : fermer, reduire,
deplacer, et surtout **le titre lu par les lecteurs d'ecran**. Les commandes redessinees doivent
donc etre de vrais boutons, atteignables au clavier, avec un `aria-label` et un focus visible, et
la fenetre doit porter un titre accessible.

⚠️ **Cible de pointeur minimale de 24 pixels** (WCAG 2.2, critere 2.5.8) pour les commandes de
fenetre. On dessine a 32 avec une zone sensible plus large.

## Palette

Imposee, reprise de l'app iOS et du site, **jamais retouchee ici** : la coherence entre les trois
produits vaut plus qu'un ajustement local.

⚠️ Deux mesures deja faites cote iOS et a ne pas refaire a l'envers : `accent` (#4682b4) sous du
texte clair ne donne que **3,61:1**, donc il est reserve aux traits, temoins et icones, ou le seuil
applicable est celui des elements non textuels. Le fond des boutons pleins portant du texte est
`accentTexte` (#3a6a93), mesure **5,04:1**.

Tout autre couple texte sur fond introduit ici doit etre **mesure et pas juge a l'oeil**.

## Langue

Interface en francais. **Aucun tiret cadratin**, regle du parc.
