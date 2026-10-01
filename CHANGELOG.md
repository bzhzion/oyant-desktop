# Changelog

Toutes les modifications notables de ce projet seront documentees dans ce fichier.

Le format est base sur [Keep a Changelog](https://keepachangelog.com/fr/1.0.0/),
et ce projet adhere au [Semantic Versioning](https://semver.org/lang/fr/).

## [Unreleased]

### Corrigé

- ⛔ **La dictée échouait sur certains microphones avec « Format audio non pris en charge : U8 »,
  à chaque appui du raccourci.** `configuration_preferee` prenait la PREMIÈRE plage mono couvrant
  16 kHz, sans jamais regarder son format d'échantillon. Un micro qui n'expose ce mono 16 kHz
  qu'en U8 (8 bits) faisait donc choisir une configuration que `demarrer` refusait ensuite de
  streamer, puisqu'il ne gérait que F32/I16/U16. **Trouvé en dictant pour de vrai**, pas en
  relecture. Deux correctifs : U8 est désormais géré (conversion triviale, origine à 128), et
  surtout `choisir_configuration` — extraite en fonction pure, testable sans carte son via
  `cpal::SupportedStreamConfigRange::new` — ne retient plus jamais une plage dont le format n'est
  pas géré, même si elle est « idéale » par ailleurs. Ça évite de refaire la même erreur sur un
  futur format qu'on ne gère toujours pas (I8, I24, I32...), qui sera alors ignoré au profit
  d'une plage moins idéale mais convertible, plutôt que de faire échouer toute la dictée.

### Ajouté

- **Le protocole d'appairage passe en version 2 : le téléphone prouve posséder sa clé, il ne se
  contente plus de la montrer.** Depuis la version 1, un appareil déjà autorisé rentrait en
  présentant à nouveau la même chaîne — un secret porteur. L'hôte envoie désormais un défi neuf
  après `Bonjour`, à TOUT le monde, connu ou non, et exige une signature Ed25519 avant de décider
  quoi que ce soit. Une signature observée une fois (dans un journal mal gardé, par exemple) ne
  redonne aucun accès : le prochain défi est différent, et ne signait rien d'avance.

  ⛔ **`verifier_entree` et `verifier_preuve` sont deux fonctions PURES de plus**, dans le même
  esprit que `decider` : la version et les bornes se refusent avant même d'envoyer un défi, la
  preuve se vérifie avant même de savoir si l'appareil est déjà connu. `decider` lui-même n'a pas
  changé de comportement, il est seulement appelé plus tard dans la connexion.

  ⛔ **Corrigé au passage : le téléphone ne recevait jamais son propre code.** `AutorisationDemandee`
  n'était renvoyée qu'à la toute fin de la connexion, une fois l'utilisateur déjà répondu sur
  l'ordinateur — ce qui rendait la consigne « comparez les deux nombres » vide de sens, puisque
  rien n'apparaissait côté téléphone pour comparer. Elle est maintenant envoyée dès qu'elle est
  décidée, et la connexion reste ouverte en attendant la réponse humaine.

  ⚠️ **Bump obligatoire de `VERSION_PROTOCOLE` (`"1"` → `"2"`)** : un téléphone de l'ancienne
  version n'enverrait jamais de preuve, et serait sinon bloqué par un délai d'attente illisible
  plutôt que par un refus clair et immédiat.

- **L'écran d'appairage et la découverte sur le réseau local.** Un téléphone peut désormais
  trouver cet ordinateur, et l'utilisateur peut l'autoriser — les deux moitiés qui manquaient.

  ⛔ **L'autorisation se donne sur la machine qui recevra les frappes.** L'écran affiche le nom
  déclaré, les **quatre chiffres**, et une consigne qui dit quoi **comparer** et non seulement quoi
  faire : « n'acceptez que si votre téléphone affiche exactement le même nombre ». Écrire
  « acceptez » seul ferait cliquer sans regarder, ce qui annulerait l'intérêt du code.

  ⚠️ **L'écouteur d'événement est posé au chargement, pas à l'affichage de la section** : une
  demande arrive quand le téléphone la lance, pas quand l'utilisateur regarde au bon endroit.

  **Deux réglages et pas un**, à dessein : « accepter les connexions d'un téléphone » **ouvre** le
  service, « autoriser depuis le réseau local » le rend **joignable**. Les fondre aurait fait
  qu'accepter un téléphone expose au réseau du même geste, sans que personne ne l'ait demandé. Le
  second porte un avertissement, **jamais sur un wifi d'hôtel ou public**.

  ⛔ **Aucun des deux n'est un réglage avancé**, bien que le second porte cet avertissement : sans
  lui **aucun** téléphone ne peut se connecter, et son absence ne produit pas d'erreur, seulement
  une recherche qui ne trouve rien. Le cacher derrière la bascule des réglages avancés rendait la
  fonctionnalité muette pour qui ne l'ouvre jamais, c'est-à-dire le cas normal. L'aide du premier
  renvoie explicitement au second, pour la même raison.

  ⛔ **L'empreinte du certificat part dans l'annonce zeroconf**, et ce n'est pas un confort : sans
  elle le téléphone devrait faire confiance au premier certificat présenté, donc au premier
  appairage — **le seul moment où il n'a encore rien à comparer**, et donc le seul où quelqu'un
  peut se placer au milieu.

  ⚠️ **L'annonce vient APRÈS l'écoute** : annoncer un port qui n'écoute pas encore ferait échouer
  la première tentative, et ce genre d'échec se lit comme « ça ne marche pas » plutôt que comme
  une course.

  ⚠️ **Un type de service déclaré** (`_oyant._tcp.local.`) et non une énumération de tout ce qui
  passe : c'est ce qui évite l'autorisation « multicast » d'Apple côté téléphone.

  ⛔ **Une réponse qui arrive trop tard le dit** : révoquer ou accepter une demande déjà expirée
  affiche « la demande avait expiré », jamais un faux succès.

- **Le serveur d'appairage, en service** (`serveur.rs`), et **l'exclusion `dead_code` posée plus
  haut a été RETIRÉE** — c'était la condition annoncée. Rien n'est plus décoratif : `clippy -D
  warnings` passe sans aucune exception.

  **TLS avec certificat auto-signé épinglé**, WebSocket, poignée de main bornée. ⚠️ **Le serveur ne
  DÉCIDE rien** : toute la règle vit dans `appairage::decider`, pure et testée à part. Les mélanger
  aurait remis la sécurité dans une boucle asynchrone, où elle ne se teste qu'avec un vrai
  téléphone — donc en pratique ne se teste pas.

  ⛔ **Le consentement passe par un canal, pas par un rappel**, et c'est ce qui rend le tout
  éprouvable **sans appareil** : quatre tests d'intégration montent le serveur sur un port libre,
  s'y connectent en **vrai TLS avec épinglage**, en **vrai WebSocket**, et jouent l'utilisateur —
  y compris quand il ne répond pas.

  ⛔ **Trois façons de ne pas dire oui, une seule conclusion** : délai expiré, interface absente,
  refus explicite. Une seule branche qui autoriserait par défaut rendrait tout le reste décoratif.
  Un test couvre le cas « personne n'écoute ».

  ⛔ **Une mauvaise version ne dérange même pas l'utilisateur** : elle est refusée **avant** la
  demande d'autorisation, et un test vérifie qu'aucune demande n'a été émise. Le solliciter pour un
  client qu'on ne sait pas interpréter, c'est l'habituer à accepter sans lire.

  ⚠️ **Le vérificateur d'épinglage du côté client vit dans les tests**, à dessein : il documente ce
  que le téléphone devra faire. Un client qui accepterait n'importe quel certificat rendrait tout
  le TLS décoratif.

  **Deux réglages, faux par défaut** : `network_enabled` et `network_all_interfaces`. ⛔ **Le filtre
  est au démarrage** — tant que le premier est faux, **aucun port n'est lié**. Un serveur qu'on
  ouvre puis qu'on « protège » plus loin est un serveur ouvert.

  ⛔ **Piège de dépendance attrapé avant le premier build** : `rustls` 0.23 prend par défaut le
  fournisseur `aws-lc-rs`, qui tire **CMake** et une chaîne C à installer sur les trois OS de la
  CI. Mesuré à `cargo fetch`, corrigé en `default-features = false` + `ring`. Le découvrir à un
  build vert sur une machine et rouge sur les deux autres aurait coûté bien plus cher.

- **Le cœur de l'appairage téléphone ↔ ordinateur** (`src-tauri/src/reseau.rs`), première tranche
  de l'étape 11. Volontairement **sans aucun serveur** : ce sont les décisions qui se testent sans
  réseau, et le serveur viendra dessus. C'est ce qui permet de prouver ces règles par des tests
  plutôt que par un téléphone sous la main.

  ⛔ **Le pire cas de ce module n'est pas une panne**, c'est quelqu'un qui tape du texte arbitraire
  dans l'ordinateur de l'utilisateur, et sa voix en clair sur le wifi. Tout en découle.

  **Trois choix du protocole de `justmakeQ` sont refusés ici**, parce que son pire cas à lui est de
  déranger une conduite de spectacle : le clair (`ws://`), l'absence de nombre à faire
  correspondre, et surtout **une identité que le client déclare lui-même**. Ce dernier signifie
  qu'une fois un appareil autorisé, **n'importe qui peut se faire passer pour lui** en renvoyant la
  même valeur. Ici l'identité est l'**empreinte d'une clé publique**, et le nom ne sert qu'à
  l'affichage.

  ⛔ **Écoute sur la boucle locale par défaut**, toutes interfaces jamais sans geste délibéré,
  règle déjà appliquée à BeamMeUp et Hublot. ⚠️ Le défaut est donc **inutilisable pour un
  téléphone, et c'est voulu** : il protège celui qui n'a rien demandé.

  **Les quatre chiffres sont dérivés, pas transmis** : les deux côtés les calculent depuis ce
  qu'ils connaissent tous les deux, sans quoi un tiers pourrait les lire et les rejouer. Un test
  vérifie que deux appareils qui demandent en même temps n'affichent pas le même nombre — c'est
  tout l'intérêt de la fonction.

  ⚠️ **Le réappairage ne duplique pas**, et c'est un test à part : un doublon rendrait la
  révocation inopérante, ce qui est pire que ne pas révoquer du tout.

  **10 tests**, ⚠️ **prouvés rouges par mutation** : forcer l'écoute sur toutes les interfaces et
  retirer la déduplication font tomber exactement les deux tests qui les protègent.

  ⛔ **Une exclusion `dead_code` datée avait été posée en tête de module**, avec ce qui la lève :
  rien n'appelait encore ces fonctions, donc `clippy -D warnings` refusait le module, **et il avait
  raison**. ✅ **Elle a été retirée dans la même livraison**, dès que le serveur a appelé
  `adresse_ecoute` et `code_visuel`. La garder « au cas où » aurait masqué la prochaine fonction
  réellement morte — et c'est exactement ce qui a permis de repérer que `empreinte_certificat`
  l'était encore, ce qui a conduit à l'exposer là où elle sert vraiment : le repli par QR.

### Corrigé

- ⛔ **L'ordinateur s'annonçait sur le réseau même quand il n'écoutait que sur la boucle locale.**
  Avec « accepter les connexions d'un téléphone » coché mais pas « autoriser depuis le réseau
  local », le téléphone **trouvait** l'ordinateur et échouait ensuite à s'y connecter. C'est le
  pire des deux symptômes : ne rien trouver est honnête et renvoie vers le réseau ou les réglages,
  trouver sans joindre envoie chercher une panne d'appairage ou de certificat qui n'existe pas.
  L'annonce est maintenant conditionnée à la même case que l'écoute.

- ⚠️ **Le commentaire de `code_visuel` décrivait un idéal et pas le code** : il affirmait que les
  deux côtés calculent le nombre chacun de leur côté, alors que **le téléphone le reçoit de
  l'hôte**, qui seul connaît le défi. Ce qui rend ce choix tenable n'est donc pas une dérivation
  symétrique mais le fait que **le canal est déjà du TLS épinglé** et que le nombre est propre à
  une connexion. Corrigé pour dire ce qui est écrit, parce qu'un commentaire faux fait raisonner
  faux sur la sécurité qu'il prétend expliquer.

- ⛔ **L'empreinte du certificat était calculée sur le PEM**, et le téléphone n'aurait donc jamais
  pu la reconnaître. Le PEM est une enveloppe de texte (en-têtes, base64, retours à la ligne tous
  les 64 caractères) dont rien ne garantit la reproduction à l'identique d'une bibliothèque à
  l'autre, et **iOS ne la fournit même pas** : `SecCertificateCopyData` rend le **DER**. Les deux
  côtés auraient calculé des valeurs différentes, et l'appairage aurait échoué en accusant
  l'épinglage, pour une raison qui n'a rien à voir avec la sécurité.

  ⚠️ **La fonction rend désormais une erreur plutôt qu'une empreinte fausse** quand le PEM ne
  contient aucun certificat : une empreinte calculée sur du vide se comparerait très bien à
  elle-même. Un test le prouve, et le serveur refuse de démarrer dans ce cas au lieu d'annoncer une
  empreinte qui ne désigne rien.

## [0.2.0] - 2026-09-25

### Added

- **Le vocabulaire se remplit tout seul depuis les substitutions.** La cible d'une substitution,
  quand elle ressemble à un terme et non à une tournure, est donnée au moteur avant la
  transcription au même titre que le vocabulaire saisi à la main.

  ⛔ **Le signal utile n'est PAS la sortie du moteur, et c'est tout l'arbitrage.** L'idée de
  départ était de récolter les acronymes et les mots à casse mixte dans le texte transcrit. C'est
  **circulaire** : la sortie ne contient que ce que le moteur a déjà su écrire, alors que les
  termes qui ont besoin d'aide sont exactement ceux qui n'y apparaissent jamais. On aurait
  renforcé précisément le cas qui n'en a pas besoin. La cible d'une substitution, elle, est un mot
  que l'utilisateur a dû corriger à la main : c'est la meilleure liste des erreurs du moteur dont
  on dispose, elle est non circulaire, et elle était déjà dans les réglages.

  ⚠️ **Le filtre est volontairement prudent, et l'asymétrie des échecs le justifie.** Oublier un
  terme ne coûte **rien** : la substitution continue de corriger le texte après coup, exactement
  comme avant. Retenir à tort une tournure de ponctuation mange le budget du prompt et biaise le
  moteur vers une expression que personne n'a prononcée. Entre les deux, on rate. D'où les trois
  critères : au plus trois mots, au plus quarante caractères, et **au moins une majuscule**, qui
  est ce qui sépare `Lévothyrox` ou `ECG` de `n'est-ce pas ?`.

  ⛔ **La fusion vit dans UN point d'entrée, `prompt_des_reglages`, et pas chez les appelants.**
  Il y en avait deux, la dictée et la ligne de commande, et un troisième aurait oublié la moitié
  du vocabulaire sans que rien ne vire au rouge. Même famille que le repli branché écran par
  écran : posé dans la fonction partagée, il ne peut plus être oublié par personne.

  ⚠️ **L'ordre décide de ce qui survit à la troncature** : le vocabulaire saisi passe devant, les
  termes déduits comblent ce qui reste sous le plafond. L'inverse ferait tomber une liste choisie
  à la main au profit d'un sous-produit.

  ✅ Trois tests, chacun **prouvé rouge par mutation** : filtre des majuscules retiré, ordre
  inversé, déduplication retirée.

  ⚠️ **Premier passage livré rouge en CI, et l'erreur est de méthode** : j'avais lancé
  `cargo test` et rien d'autre, alors que la CI lance aussi `cargo fmt --check` et `cargo clippy
  --all-targets`. Clippy refusait l'affectation de champ après `Default::default()` dans deux des
  nouveaux tests, corrigée en syntaxe de mise à jour de structure. **Lancer les tests ne vérifie
  pas ce que la CI vérifie** : c'est la liste des étapes du workflow qui fait foi, pas l'habitude.

- **Un test qui garde l'historique désactivé par défaut**, `l_historique_est_desactive_par_defaut`,
  **prouvé rouge** en remettant la valeur de 20 qui avait cours jusqu'au 2026-09-18.

  ⛔ **Le défaut était déjà à zéro depuis le 2026-09-18, mais rien ne le protégeait.** Sans ce
  test, remettre une valeur « serviable » ne casse rien : l'application marche mieux du point de
  vue de celui qui fait le changement, et le défaut de confidentialité ne se voit nulle part. C'est
  exactement le genre de régression qu'aucune relecture n'attrape, parce qu'elle ressemble à une
  amélioration. Le test vérifie aussi que `normaliser()` ne le relève pas en douce, comme il le
  faisait quand la borne basse valait 1.

- **`scripts/verifier-depot-public.py`, branché dans la CI et prouvé rouge.** Il refuse toute
  référence à un dépôt privé dans les fichiers suivis par git, donc exactement ce qui est publié.

  ⚠️ **Retirer ces références ne protège pas de leur retour, seul ce contrôle le fait** : elles
  sont justes du point de vue de quelqu'un qui a les deux dépôts ouverts, et c'est précisément le
  point de vue de celui qui écrit le commentaire. Le contrôle a d'ailleurs trouvé trois occurrences
  que ma relecture avait manquées.

### Changed

- **`actions/checkout` et `actions/setup-node` passent en v7** dans les workflows : les versions
  posées déclaraient `using: node20`, déprécié et déjà forcé sur Node 24 par GitHub. Les quatre
  changements de rupture de ces majeures ont été lus et confrontés au parc, aucun ne s'y applique,
  et les 11 runners de l'org sont en 2.336.0 ou mieux, au-dessus du minimum 2.327.1 qu'exigent
  `checkout` v5 et `setup-node` v5. Vérifié par un build iOS réel avant propagation.

- ⛔ **Le produit s'appelle désormais Oyant, et ce n'est pas un choix esthétique.** Une
  application **« Dictum - private voice to text »** existe déjà sur l'App Store (Factory Design
  LLC, identifiant `6759581003`), et ce n'est pas une homonymie : même nom, même sous-titre
  (« Dictation without the cloud »), même thèse (« No server, no upload, no account »), même
  traitement sur l'appareil, même gratuité, mêmes plateformes. Un utilisateur ne peut pas
  distinguer les deux produits, et un moteur de réponse encore moins.

  ⚠️ **Le coût augmente chaque semaine, d'où le fait de le faire maintenant** : le dépôt est
  public depuis trois jours, un seul tag est sorti, et l'application mobile n'a jamais été
  publiée. Attendre aurait signifié construire davantage de référencement sur un nom qui envoie
  les lecteurs chez quelqu'un d'autre.

  **Oyant** est un terme de droit, participe présent d'« ouïr » : dans une reddition de compte,
  le *rendant* présente sa gestion et l'*oyant* est celui qui **entend** le compte. Le nom dit
  donc la fonction, dans le vocabulaire des professions auxquelles ce logiciel s'adresse.
  Vérifié avant d'être retenu : **aucune marque en classe 9 ni 42**, INPI comme TMview.

  ✅ **Les données ne sont pas perdues.** L'identifiant passe de
  `org.breizhzion.dictum.desktop` à `org.breizhzion.oyant.desktop`, ce qui déplace le répertoire
  où vivent les modèles, l'historique et les réglages. Une reprise au premier lancement déplace
  l'ancien répertoire vers le nouveau. ⛔ **Sans elle, le renommage orphelinerait jusqu'à 3,1 Go
  de modèles** et forcerait un retéléchargement.

  ⛔ **La reprise n'écrase jamais ce qui existe déjà** : quelqu'un qui aurait lancé la nouvelle
  version avant de migrer a des réglages neufs, et les remplacer par les anciens serait une perte
  silencieuse. Prouvé rouge par mutation, comme le déplacement lui-même.

  ✅ **Les objets R2 ont été copiés et le code repointé le 2026-09-25** : le moteur Vulkan est
  servi depuis `oyant-desktop/moteurs/oyant-moteur-windows-x64-vulkan-b5130.zip`.

  ⛔ **Les anciens objets n'ont PAS été supprimés, et ne le seront pas** : le binaire 0.1.1 déjà
  installé va chercher son moteur à l'ancienne adresse, et un objet R2 ne coûte presque rien.
  **On copie vers le nouveau préfixe, on ne déplace jamais.**

  ⚠️ **L'ETag est le mauvais instrument pour vérifier une copie R2.** Un objet déposé en
  multipart porte un ETag en `-N` que la copie recalcule en MD5 simple : deux objets identiques
  s'annoncent alors « différents », et on croit avoir cassé quelque chose. La preuve est venue
  des empreintes que les manifestes portent déjà — MD5 pour les modèles, SHA-256 pour le moteur.

  ⚠️ **La lecture publique du bucket rend 403**, il porte une règle d'origine : vérifier par
  l'API S3, ce qui évite au passage de retélécharger 2,7 Go pour un contrôle.

  ⚠️ **Ce fichier n'a pas été renommé non plus**, et c'est volontaire : ce qui est sorti sous le
  nom Dictum doit continuer à le dire. Réécrire l'historique n'est pas le documenter.

  ⚠️ **Correction d'une affirmation de cette entrée** : j'ai d'abord écrit que GitHub ne
  redirigeait pas le dépôt renommé, sur la foi d'un 404 mesuré dans les minutes qui suivaient.
  **C'est faux** : la redirection fonctionne, elle n'est simplement pas immédiate. Corriger les
  liens entrants reste utile, mais pour une autre raison — elle tombe si quelqu'un reprend
  l'ancien nom.

  ⚠️ **« Oyant » commence par une voyelle, « Dictum » par une consonne.** Un remplacement de
  chaîne ne connaît pas cette différence : il a produit « de Oyant » et « que Oyant » à 21
  endroits, dont trois textes affichés dans les réglages. Corrigé séparément, parce qu'aucun
  compilateur ne voit ce défaut-là.

- **Le compteur du vocabulaire ne prétend plus compter ce qu'il ne compte pas.** Il annonçait
  « X caractères sur 600 utilisés » en ne mesurant que les termes saisis, alors que les termes
  déduits consomment désormais le même budget.

  ⛔ **Le filtre n'a pas été recopié dans l'interface pour rendre le nombre exact**, et c'était la
  tentation : deux implémentations de la même règle divergent, et rien ne le signale. Le libellé
  dit donc précisément ce qu'il mesure, et une phrase explique que les substitutions alimentent
  aussi cette liste. Un libellé exact vaut mieux qu'un nombre faussement précis.

- **`scripts/mesurer-vocabulaire.py`, pour mesurer ce que le vocabulaire apporte vraiment** sur un
  vrai enregistrement, plutôt que de le supposer.

  ⛔ **Le même fichier audio sert aux deux passes.** Comparer deux enregistrements différents ne
  mesurerait pas le vocabulaire mais la façon de parler du second.

  ⛔ **Il mesure aussi le SUR-BIAIS, et c'est le point le plus important.** Le texte à lire
  contient deux mots volontairement **absents** du vocabulaire et phonétiquement proches d'un
  terme présent, `Lévétiracétam` près de `Lévothyrox` et `Valbonne` près de `Villeurbanne`. S'ils
  se font aspirer vers leur voisin, le mécanisme corrige à tort et il ne faut pas le garder. Une
  mesure qui ne regarde que le gain passerait entièrement à côté de ce risque, qui est précisément
  celui qui compte sur des professions tenues au secret.

  ⚠️ **Le texte à lire vit dans le script**, à côté du baromètre qui le note, pour que les deux ne
  divergent jamais. Ce sont des phrases naturelles et non une liste de mots : on lit une liste
  autrement qu'on dicte, et c'est la dictée qu'on mesure.


- **Un vocabulaire donné au moteur AVANT la transcription** (`vocabulary` dans les réglages, une
  zone de texte à raison d'un terme par ligne). Les termes partent en `--prompt` avec
  `--carry-initial-prompt`, pour que la reconnaissance écrive du premier coup les noms propres et
  les termes de métier qu'elle écorche.

  ⛔ **Ce n'est PAS une liste de substitutions, et les confondre était l'erreur que j'allais
  faire.** Une substitution corrige le texte après coup, de façon exacte et sans discernement ; le
  vocabulaire est un **biais souple** appliqué avant, donc **rien ne peut être remplacé de
  travers**. Les deux blocs de l'écran sont présentés dans l'ordre du traitement pour que la
  différence se voie.

  ✅ **Recette reprise de `teelap/local-dictation`**, qui la sépare en trois couches (indices de
  décodage, puis passe floue, puis remplacements littéraux) là où j'allais greffer une correction
  approximative sur les substitutions existantes. painteau a demandé de regarder ce que font les
  projets existants avant d'écrire : c'était le bon réflexe, et c'est une leçon du parc que je
  n'avais pas appliquée.

  ⚠️ **La correction approximative après coup est délibérément remise à plus tard**, et cet ordre
  est l'apport principal de l'étude : la couche de biais ne peut pas se tromper, donc elle se
  livre sans risque et on mesure ensuite ce qu'il reste à réparer. La faire d'abord aurait fait
  payer le coût de Beider-Morse et de ses fichiers de règles sans savoir s'il se justifie.

  ⛔ **`--carry-initial-prompt` n'est pas optionnel** : sans lui le prompt ne vaut que pour la
  première fenêtre de trente secondes, donc le vocabulaire se perdrait en cours de route et le
  défaut n'apparaîtrait que sur les longues dictées, rarement et sans rapport apparent avec la
  longueur.

  ✅ **Vérifié avant d'écrire la fonction : le prompt ne fuit pas dans la sortie**, testé sur du
  silence absolu, du bruit très faible et un sinus audible avec le modèle Small. C'était le risque
  à lever : voir la liste de ses patients apparaître dans un compte rendu serait un défaut
  inacceptable. ⚠️ Le prompt change en revanche **ce qui** est halluciné sur du non-parlé, les
  trois cas rendant « L'Ontario » au lieu de « ... », ce qui confirme l'utilité du seuil de silence
  qui empêche d'envoyer au moteur un enregistrement sans parole.

  ⚠️ **Le plafond de 600 caractères est un substitut assumé** : le moteur compte en jetons et on ne
  peut pas les compter sans embarquer son tokeniseur. La pratique documentée est qu'au-delà
  d'environ 200 jetons un prompt dégrade la transcription au lieu de l'aider. **La troncature tombe
  entre deux entrées, jamais au milieu d'un mot** (prouvé rouge par mutation) : un prompt coupé sur
  « Kowal » biaiserait le moteur vers un fragment inexistant, ce qui est pire que de ne pas donner
  l'entrée.

### Fixed

- **Le README laissait entendre que l'historique conserve par défaut**, alors que c'est l'inverse.
  L'inexactitude allait dans le sens qui nous dessert : rien n'est écrit tant que l'utilisateur ne
  l'active pas, et c'est un bien meilleur argument que ce qui était écrit.



- ⛔ **Le README annonçait « étape 2 sur 13, le binaire ne dicte rien encore » sur un dépôt public
  qui distribue une 0.1.1 qui dicte.** C'était la première phrase que lisait un visiteur, et elle
  était fausse. Le dépôt est passé public sans que ce fichier soit touché.

  Il est réécrit pour quelqu'un qui arrive de l'extérieur : ce que le logiciel fait, comment
  l'installer sur les trois systèmes, ce qu'il ne fait pas, où les dictées sont écrites sur le
  disque, et seulement ensuite la partie développement.

- ⛔ **Sept endroits du dépôt public désignaient encore des documents d'un dépôt PRIVÉ comme source
  d'autorité**, dont deux dans le code et trois dans les workflows. Un lecteur tombait sur une
  référence qu'il ne pouvait pas suivre, et la structure interne fuyait sans aucun bénéfice. Le
  README renvoyait en plus à un dépôt devenu privé et archivé.

  Chaque explication a été rendue **autonome** : quand un choix technique mérite d'être justifié,
  le raisonnement s'écrit sur place.

## [0.1.1] - 2026-09-21

### Added

- **Le dépôt est public et porte la licence BZ-1.1**, avec un `NOTICE.md` qui reproduit les
  licences MIT de whisper.cpp, des modèles Whisper d'OpenAI et les termes de redistribution du
  runtime Visual C++. Ce n'est pas une formalité : l'installateur Windows **embarque** un moteur
  whisper.cpp compilé, et la licence MIT exige que sa notice voyage avec **chaque copie, y compris
  binaire**.

- **Téléchargement public sur `dl.breizhzion.com/dictum-desktop/`**, selon la convention du parc :
  un nom fixe que le site vise, une copie versionnée qu'un gestionnaire de paquets épingle, et un
  `latest.json` qui évite de redéployer le site à chaque version.

  ⛔ **Sans ça, personne sous Windows ne pouvait obtenir Dictum**, et la 0.1.0 est sortie dans cet
  état : le dépôt était privé, donc sa release GitHub n'était pas téléchargeable, et le site
  pointait encore vers l'ancien dépôt et sa 1.7.0. Le canal Linux, lui, marchait par apt et
  pacman.

  ⚠️ La copie versionnée n'est pas une précaution théorique : un manifest winget porte le SHA256
  de l'installateur, donc écraser le nom fixe invalide l'empreinte de **toutes** les versions déjà
  publiées, et le canal casse **à la release suivante** et pas à la publication.

- **Le job qui choisissait un runner maison a disparu, et avec lui `GH_RUNNER_ADMIN_TOKEN`.** Il
  n'existait que parce que les minutes d'un dépôt privé sont facturées, macOS dix fois ; sur un
  dépôt public les runners GitHub sont gratuits. Le gain qui compte n'est pas les 120 lignes en
  moins, c'est qu'un jeton **administrateur de l'organisation** ne dort plus dans les secrets d'un
  dépôt public.

### Changed

- **L'historique git a été écrasé en un commit initial** au moment de rendre le dépôt public. Le
  balayage de l'ancien historique était propre (aucune clé, aucun motif de jeton, `.scratch/` bien
  ignoré), donc c'était un choix de présentation et non une nécessité de sécurité. Il coûtait peu
  ici : cinq jours et 52 commits, et surtout **le raisonnement ne vit pas dans les messages de
  commit mais dans ce fichier**, qui survit.


## [0.1.0] - 2026-09-21

### Added
- **La version du tag est posée dans `tauri.conf.json` avant la construction**
  (`scripts/poser-version.py`), parce que **Tauri ne lit pas `build.rs`**. Le binaire dérivait déjà
  sa version du tag, le paquet non.

  ⛔ **Trouvé en préparant le premier tag, et la panne aurait été entièrement silencieuse** : le
  contrôle « le binaire annonce la version du tag » serait passé au vert, le binaire disant bien
  `0.1.0`, pendant que le `.deb` publié se serait appelé `Dictum_0.0.0_amd64.deb`. `apt` aurait
  enregistré `0.0.0` et **n'aurait jamais proposé la moindre mise à jour**, quel que soit le nombre
  de tags posés ensuite. Rien n'échoue, ni à la construction, ni à la publication, ni à
  l'installation. Le paquet pacman n'avait pas le défaut, sa version étant passée explicitement,
  ce qui aurait fait diverger les deux dépôts sans rien signaler non plus.

  ⛔ **Et ce correctif a cassé le garde-fou censé le protéger, au premier tag, sur les trois
  systèmes** : poser la version dans le manifeste **salit l'arbre**, donc le `git describe
  --dirty` de `build.rs` rendait `0.1.0-dirty`. La construction force désormais `DICTUM_VERSION`,
  que `build.rs` prévoit comme premier niveau de résolution. Motif exact du parc : **tester
  l'artefact que l'édition produit, jamais une chaîne qu'une autre édition de la même passe
  introduit.**

  ⚠️ Hors tag le manifeste n'est pas touché : un `workflow_dispatch` doit produire exactement ce
  qu'une construction locale produit. Et le script **refuse une version non semver**, un nom de
  branche passé par mégarde coûtant sinon toute la compilation avant que Tauri ne s'en aperçoive à
  l'empaquetage.


- **Dictum se distribue sur pacman**, à côté de l'apt : `packaging/pacman/dictum-bin/PKGBUILD`,
  `scripts/preparer-pkgbuild.py`, un job `pacman` dans `release.yml`, et une clé dédiée à commande
  forcée sur axolotl qui ne peut publier qu'un paquet nommé `dictum-bin`.

  ✅ **La chaîne est prouvée de bout en bout le 2026-09-21**, pas seulement écrite : paquet
  construit dans un conteneur Arch, publié par la clé restreinte, puis **réellement installé**
  depuis `pacman.breizhzion.com` avec `SigLevel = Required DatabaseRequired`.

  ⛔ **`repo-add -s` signe la BASE, jamais les paquets.** Sans un `gpg --detach-sign --no-armor`
  explicite, la publication réussit sans le moindre avertissement et c'est l'utilisateur qui
  ramasse la panne, `pacman -S` refusant le paquet pour signature absente. Le piège était déjà
  payé et documenté sur `justmakeq`, et je l'ai quand même repris à zéro.

  ⚠️ **La clé ne signe QUE son paquet et ne passe QUE lui à `repo-add`**, là où `justmakeq` boucle
  sur `*.pkg.tar.zst` : une clé restreinte ne doit pas pouvoir réécrire l'entrée d'un autre
  produit du dépôt.

  ⚠️ **Les quatre dépendances Arch sont vérifiées contre les dépôts officiels à chaque
  construction, et la liste est DÉRIVÉE du PKGBUILD** plutôt que recopiée dans le workflow. Un nom
  qui change en amont produirait sinon une installation qui réussit et une application qui refuse
  de démarrer en nommant un `.so`.

  ⛔ **`makepkg` refuse net un PKGBUILD en CRLF** (« contains CRLF characters and cannot be
  sourced »), et Python traduit `\n` en `\r\n` en écriture texte sur Windows. Le défaut ne se voit
  sur aucune machine Linux, donc pas en CI : c'est le premier essai réel qui l'a sorti. Corrigé
  dans le script **et** dans `.gitattributes`, qui ne couvrait pas un nom de fichier sans
  extension.

- **La première publication a été faite pour de vrai, sur les deux dépôts**, parce qu'une première
  publication n'est pas une mise à jour : `dictum` et `dictum-bin` étaient absents des index, les
  cinq paquets apt et le paquet pacman déjà en place ont survécu à l'opération. C'est exactement
  là que `justmakeq` avait disparu de l'index apt la fois précédente. Aucune étape de la chaîne ne
  lit un objet distant pour le réécrire : `reprepro includedeb`, `repo-add` et `gh release create`
  créent tous ce qui manque.


- **Le raccourci global fonctionne sous Linux sans une ligne de code spécifique**, vérifié sous
  X11 : `global-hotkey`, sous le greffon Tauri, a une implémentation X11 qui pose un `XGrabKey`.
  Les quatre façons d'envoyer la combinaison le déclenchent, et la chaîne va jusqu'au cœur.

  ⚠️ **L'astuce qui rend ce test possible sans matériel** : le raccourci déclenche `commencer`, qui
  vérifie le moteur et le modèle **avant** d'ouvrir le microphone. Sur une machine qui n'en a
  aucun, la dictée s'arrête en le disant, et cette ligne prouve que la touche est arrivée. Sans
  ça, prouver le raccourci demanderait une carte son, un modèle de plusieurs centaines de
  mégaoctets et quelqu'un pour parler.

  ⛔ **Deux traces ajoutées, à l'enregistrement et à chaque déclenchement, et elles restent.** Sans
  elles, un raccourci qui ne répond pas est **indistinguable** d'un raccourci jamais enregistré :
  les deux se manifestent par une application parfaitement silencieuse. C'est ce qui a rendu le
  premier échec indiagnosticable.

  ⚠️ **Ce premier échec était un défaut du test, pas du code** : j'attendais douze secondes alors
  que l'initialisation de la webview en prend davantage sur cette machine, donc j'envoyais la
  touche avant l'enregistrement. Le test attend désormais **réellement** la ligne d'enregistrement
  au lieu de dormir un temps arbitraire. Une temporisation fixe est soit trop courte sur une
  machine chargée, soit du temps perdu sur une machine rapide, et dans le premier cas elle fait
  conclure à un défaut qui n'existe pas.

- **L'injection au curseur fonctionne sous Linux**, et c'est vérifié : le texte est réellement
  arrivé dans une autre application, accents, ligature `œ`, guillemets français, hangeul et emoji
  compris. Plus simple que sous Windows, où l'emoji impose de découper en paires de substitution
  UTF-16 : `enigo` s'en charge.

  ⛔ **Le test relit ce qui est arrivé, il ne croit pas le code de retour.** `enigo` rend un succès
  dès que le serveur X accepte les événements, exactement comme `SendInput` : un test qui se
  contenterait de ça serait vert sans que rien ne soit arrivé nulle part, ce qui est arrivé trois
  fois le 2026-09-17. Le montage est dans `scripts/essai-injection-x11.sh` : serveur X virtuel,
  `xterm` qui note tout ce qu'il reçoit dans un fichier, puis comparaison. ⚠️ **Aucun écran ni
  personne devant**, donc rejouable en intégration continue.

  ⚠️ **La sonde de refus n'est pas la même que sous Windows, parce que le risque n'est pas le
  même.** Là-bas la saisie part dans le vide quand une fenêtre ne peut pas la recevoir, d'où la
  lecture du titre ; ici quand il n'y a **aucun serveur d'affichage**, l'état ordinaire d'une
  machine de compilation, ou sur un Wayland sans portail `RemoteDesktop`. La sonde Linux nomme
  donc la **session** et rend vide quand il n'y a rien à viser, ce qui fait échouer le test au lieu
  de le laisser passer là où rien ne pouvait arriver.

  ⚠️ **Sans pause demandée le texte part d'un bloc**, nettement plus rapide que touche par touche ;
  le découpage ne sert qu'aux applications qui perdent des caractères. Et la restauration de
  fenêtre rend `None` sous Linux **volontairement** : ces environnements ne laissent pas une
  application se remettre au premier plan à volonté, et ce `None` fait prendre le chemin « pas de
  cible à restaurer », donc l'injection va où est le focus. C'est correct, pas un repli.

### Fixed
- ⛔ **Le paquet Debian ne déclarait pas `libasound2`, dont le binaire dépend pourtant.** Constaté
  le 2026-09-20 en installant réellement sur `fugu` : `ldd` montre `libasound.so.2` parmi les
  bibliothèques du binaire, tirée par `cpal`, alors que le `Depends:` du paquet n'en dit rien.
  ⚠️ **Tauri déduit les dépendances des bibliothèques qu'il connaît, pas de celles que nos caisses
  tirent.** Sur une machine où elle manque, `apt install` **réussit** et l'application refuse de
  démarrer en nommant un fichier `.so` : l'échec arrive au premier lancement, loin de sa cause.
  ⚠️ On déclare `libasound2`, le nom historique, et non `libasound2t64` : depuis la transition
  `time_t` de Debian c'est le second qui existe réellement, mais il **fournit** le premier.
  Déclarer l'ancien nom couvre les deux générations, l'inverse casserait sur toute distribution
  restée en arrière.
- ⛔ **L'intégration continue était rouge depuis le 2026-09-17, et dix-sept poussées s'y sont
  succédé sans que personne ne regarde.** C'est exactement la leçon que ce dépôt porte déjà : une
  CI rouge finit ignorée.
  La cause précède le garde-fou de `build.rs`, qui n'a fait que remplacer un message obscur de
  Tauri par un message clair : `src-tauri/moteur/` est **ignoré par git** depuis l'étape 6,
  `src-tauri/runtime/` depuis l'étape 7, et **aucun workflow ne les fabriquait**. Une organisation
  à deux vitesses, où le code est versionné et les entrées binaires produites par des scripts que
  personne n'avait raccordés à la chaîne.
  Les deux scripts sont désormais appelés **avant toute étape cargo**, dans `ci.yml` comme dans
  `release.yml`.
- **Les ressources embarquées passent dans `tauri.windows.conf.json`.** Elles étaient déclarées
  sans condition alors qu'elles n'ont de sens que sur Windows, ce qui faisait échouer les trois
  cibles. ⚠️ Et le garde-fou de `build.rs` lit `CARGO_CFG_TARGET_OS` et non `cfg!(windows)` : un
  script de construction tourne sur la machine **hôte**, donc `cfg!` y décrit l'hôte et pas la
  cible.

### Added
- **Linux a son moteur**, et l'amont le publie : `whisper-bin-ubuntu-x64.tar.gz` et sa variante
  ARM64 vivent dans la même publication que les archives Windows, contrairement à ce que ce projet
  supposait. ⚠️ **Rien à construire**, mais une autre extension d'archive.
  ⛔ **Un `tar` n'est pas un `zip`, et trois différences comptent** : il porte les **droits**, sans
  lesquels `whisper-cli` arrive sans son bit exécutable ; il contient des **liens symboliques**
  entre bibliothèques, sans lesquels celles-ci sont introuvables à l'exécution ; et il peut viser
  **hors** du répertoire cible exactement comme un zip, donc les mêmes garde-fous y sont écrits
  plutôt que délégués à la bibliothèque, où ils seraient invisibles à la relecture.
  ⚠️ `LD_LIBRARY_PATH` est l'équivalent Linux du `PATH` posé sous Windows, mais il vise un autre
  répertoire : celui du **moteur**, puisque sous Linux un exécutable ne trouve pas les `.so` posés
  à côté de lui, là où Windows cherche les DLL dans le répertoire du binaire lancé.
  ⛔ **Aucun choix de moteur n'est proposé sur Linux** : l'amont n'y publie ni CUDA ni Vulkan, et
  offrir une accélération qui n'existe pas serait le même mensonge que d'annoncer une carte
  graphique sur un calcul fait par le processeur.

### Added
- **L'échec d'une dictée est désormais visible sans ouvrir la fenêtre**, par une notification du
  système. ⛔ **C'était le défaut le plus gênant du produit** : Dictum vit dans la zone de
  notification, donc sa fenêtre est fermée la plupart du temps ; une dictée qui échouait se
  manifestait par une icône revenant au repos et un texte qui n'arrivait pas, **sans aucun moyen de
  savoir pourquoi**. C'est exactement ce que WhimprFlow a dû corriger, et qu'on avait au même
  endroit tout en se félicitant de ne pas l'avoir.
- **La température est branchée sur le moteur** (`--temperature`), après vérification que
  `whisper-cli` l'accepte. Elle était affichée et inerte.

### Changed
- **Le modèle par défaut passe de `medium` à `small`** (arbitré par painteau le 2026-09-20).
  ⚠️ **Le défaut du modèle est lié à celui du moteur** : une installation neuve calcule sur le
  processeur embarqué, et c'est là que la différence se voit. Mesuré sur la machine, même extrait :

  | Modèle | Téléchargement | Sur le processeur |
  |---|---|---|
  | `small` | 487 Mo | **7,2 s** |
  | `medium` | 1,5 Go | 17,9 s |

  Le défaut sert la **première minute** de quelqu'un qui découvre, pas le meilleur résultat
  possible : 1,5 Go à télécharger avant de pouvoir dire un mot, puis vingt secondes d'attente, est
  un mauvais premier contact. Un test garde la **raison** et non la valeur : aucun modèle du
  catalogue ne doit peser moins que celui proposé d'emblée.

### Removed
- ⛔ **Cinq réglages étaient affichés en ne pilotant rien, et deux étaient à « oui » par défaut.**
  Le pire était **« Mises à jour automatiques »**, cochée, promettant de tenir le logiciel à jour
  alors que rien n'existait derrière : une promesse de sécurité est le pire endroit où laisser un
  réglage qui ment. Retirés de l'écran, **pas du fichier** : une configuration existante n'est pas
  cassée et ils reviendront quand quelque chose les lira. Concernés : mises à jour automatiques,
  niveau de journal, pause des médias, transcription en direct.
- Le marqueur de construction `origine.txt` ne part plus avec l'installation.

### Fixed
- ⛔ **Le contrôle des réglages ne vérifiait que la moitié de la règle.** Il refusait qu'un réglage
  déclaré ne pilote rien, mais pas qu'un réglage **inerte soit affiché**. C'est ce qui a laissé
  passer les cinq ci-dessus, dont je n'avais repéré que deux à l'œil. Le contrôle couvre maintenant
  les deux sens.
- **Étape 8 : les options de texte.** Nouveau module `texte.rs`, entièrement de fonctions **pures**,
  plus `historique.rs`.

  ⛔ **L'ORDRE des transformations est tout le sujet, et il n'est pas interchangeable.**
  Substitutions, puis majuscule, puis typographie, puis encadrement. Les substitutions **doivent**
  passer avant la typographie : celle-ci remplace des espaces ordinaires par des insécables, et une
  règle qui cherche « n'est-ce pas ? » ne trouverait plus rien. Prouvé par mutation : en inversant,
  le texte ressort inchangé. Et la majuscule passe après les substitutions, puisqu'une substitution
  peut changer le premier mot.

  ⛔ **La typographie française est une machine à faux positifs**, et c'est là qu'est le travail.
  Une règle naïve « insécable avant deux-points » transforme `14:30` en `14 :30` et `https://` en
  `https ://`. Les heures, les adresses et `C:\` sont donc exclues explicitement, une ponctuation
  répétée (`?!`) ne prend l'espace que devant la première, et une insécable déjà posée n'est pas
  doublée. ⚠️ **Espace insécable ordinaire `U+00A0` et non l'espace fine `U+202F`** que la
  typographie soignée préfère : elle manque à beaucoup de polices et s'afficherait en carré vide,
  or on injecte dans n'importe quelle application, terminal compris.

  **Substitutions** : ajoutées aux réglages, elles manquaient à la structure alors que le contrat
  les listait depuis le départ. ⚠️ **Insensibles à la casse par défaut**, parce que le moteur met
  une majuscule en début de phrase de sa propre initiative : une règle sensible cesserait de
  s'appliquer selon la place du mot dans la phrase. Éditeur ligne par ligne dans les réglages.

  **Historique** : dans le profil local, écrit par fichier temporaire puis renommage.
  ⚠️ **Il retient le texte BRUT et non le texte transformé** : c'est ce qui a été dit qui a valeur
  de trace, une substitution étant un choix d'affichage.

### Changed
- ⛔ **La pause entre les touches passe de 50 ms à ZÉRO, et son libellé ne mentait plus.**
  Constaté par painteau au premier essai réel, le 2026-09-18 : la dictée « a mis un peu de temps »
  sans qu'il puisse en identifier la cause. C'était ce réglage, appliqué **après chaque
  caractère** : une phrase de cent caractères mettait **cinq secondes** à s'écrire.
  Pire que la valeur, le libellé : il annonçait « le temps que Dictum attend **avant** de taper,
  pour laisser la fenêtre visée redevenir active », c'est-à-dire autre chose que ce que le code
  faisait. **Un réglage qui décrit autre chose que son effet coûte plus qu'un réglage absent**,
  puisqu'on le tourne dans le mauvais sens en croyant bien faire. Il s'appelle maintenant « Pause
  entre les touches », il ne sert qu'aux applications qui perdent des caractères arrivant trop
  vite, et l'attente que l'ancien libellé promettait existe bien mais ailleurs, dans la
  restauration du focus.

- ⛔ **Le modèle ne se choisit plus que dans l'écran de transcription**, avec son poids, son état et
  son téléchargement. Il avait **en plus** une liste déroulante dans les réglages, et le défaut
  s'est manifesté exactement comme la règle du menu de l'icône l'annonce depuis le début : painteau
  a dicté **sans savoir avec quel modèle**, parce que la sélection n'était pas là où il la
  cherchait. ⚠️ **Un réglage présent à deux endroits et un réglage absent de celui où on le cherche
  sont le même défaut vu des deux côtés.**
  La liste des modèles devient donc un **choix**, au même motif que celle des moteurs : cocher
  télécharge si besoin puis rend le modèle actif, et l'option retenue porte la même mise en avant
  que sa voisine, bordure accentuée plus mention « Utilisé ». ⚠️ Cette mention passe **devant**
  l'état de téléchargement : « Vérifié » sur trois lignes ne dit pas laquelle sert. Un test refuse
  désormais que `model` ou `compute` réapparaisse dans l'écran de réglages.

- ⛔ **L'historique est désactivé par défaut, et il ne pouvait pas l'être.** Sa borne basse valait
  `1`, donc le réglage acceptait `0` à l'écriture et le cœur le ramenait à `1` **en silence** : une
  transcription restait conservée quoi qu'on demande. Borne corrigée, défaut passé de 20 à **0**, et
  **mettre 0 efface ce qui était gardé** — un réglage « non » qui laisse le fichier en place ne
  désactive rien du passé. C'est le fichier le plus personnel que Dictum écrit.

### Added
- **Étape 7 : les trois briques de la dictée.** Capture microphone (`audio.rs`), raccourci global
  (`raccourci.rs`), injection au curseur (`injection.rs`), assemblées par `dictee.rs`.

  ⛔ **Le raccourci se TIENT, il ne se presse pas** : on parle tant que la touche est enfoncée,
  relâcher déclenche la transcription. Une bascule laisserait un microphone ouvert sans que rien
  ne le rappelle, dans une application dont la fenêtre n'est même pas visible.

  ⛔ **Whisper n'accepte QUE du 16 kHz mono**, et presque aucun microphone ne capture à cette
  fréquence. La conversion **moyenne chaque fenêtre au lieu de décimer** : prendre un échantillon
  sur trois pour passer de 48 à 16 kHz replie tout ce qui dépasse 8 kHz dans la bande utile, et
  les fricatives (`s`, `f`, `ch`) y ont justement de l'énergie. On demande quand même 16 kHz à la
  carte quand elle sait le faire, une conversion évitée ne pouvant pas dégrader le signal.

  ⛔ **L'injection passe par `SendInput` en mode Unicode, jamais par des codes de touches
  virtuelles** : un code de touche dépend de la disposition du clavier, donc un `a` tapé en AZERTY
  programmé en QWERTY ressort en `q`. Le mode Unicode envoie le **caractère**, accents compris.
  ⚠️ **Un caractère hors du plan de base occupe deux unités UTF-16**, chacune dans son propre
  événement : le traiter comme une seule le ferait disparaître en silence.

  ⚠️ Le réglage **microphone manquait** alors que le contrat le liste ; il est ajouté, et la liste
  est remplie en interrogeant le système. **Vide veut dire « celui du système »**, seul choix qui
  suive l'utilisateur quand il branche un casque.

  ⚠️ Le raccourci **se réenregistre à l'enregistrement des réglages** : sans ça il ne changerait
  qu'au prochain lancement, ce qui se lit comme « le réglage ne marche pas ». Un raccourci déjà
  pris par une autre application le dit **au moment où on le choisit**.

  ⚠️ Les problèmes de dictée remontent par un **événement** et jamais par une boîte de dialogue :
  une modale volerait le focus de l'application dans laquelle on est en train d'écrire.

- **« Dicter » entre dans le menu de l'icône**, première action utile à y figurer depuis que la
  règle « ce menu ne porte que des actions » a été posée.

  ⚠️ **Une entrée de menu ne peut pas se tenir enfoncée**, donc elle bascule, contrairement au
  raccourci. Ce qui rendait une bascule dangereuse (un microphone ouvert sans rien pour le
  rappeler) est couvert par deux choses qui existent maintenant : l'icône change d'état, et la
  durée maximale coupe l'enregistrement toute seule. Le libellé passe à **« Arrêter la dictée »**
  pendant l'enregistrement : une entrée figée qui arrête la dictée une fois sur deux serait un
  menu qui ment.

  ⛔ **Le problème que le raccourci n'a pas : ouvrir le menu fait perdre le focus.** Le texte
  serait injecté dans notre propre fenêtre. On retient donc la fenêtre visée au démarrage, en
  parcourant l'ordre d'empilement plutôt qu'en lisant la fenêtre active, qui est la nôtre à cet
  instant. ⚠️ **Le raccourci, lui, ne retient rien** : il ne touche pas au focus, et forcer un
  retour écraserait un changement de fenêtre fait exprès pendant qu'on parle.

  ⛔ **Si le focus ne revient pas, on n'écrit RIEN : le texte va au presse-papiers.**
  `SetForegroundWindow` est bridé par Windows et le droit obtenu par le clic sur l'icône **expire**,
  donc l'échec est un cas ordinaire ; et la fenêtre visée vient d'une heuristique, qui peut
  désigner une surcouche. Pour la fonction la plus intrusive du produit, **écrire dans la mauvaise
  fenêtre est pire que ne pas écrire** : une phrase dictée peut atterrir dans une conversation, un
  terminal ou un champ de mot de passe. Le message dit alors quoi faire, et non ce qui a échoué,
  puisque le geste qui reste est un simple Ctrl+V.

- **Les bips de début et de fin sonnent** (arbitré par painteau le 2026-09-17). Les quatre réglages
  qui les décrivaient (activation, fréquence, durée) pilotent désormais quelque chose.

  ⛔ **Le bip de début se joue AVANT d'ouvrir le microphone, et l'ordre a une conséquence
  fonctionnelle, pas esthétique.** Joué en parallèle, Dictum s'enregistrerait lui-même, et un bip
  capté dépasse le seuil de silence : un appui accidentel déclencherait alors une transcription au
  lieu d'être reconnu comme « personne n'a parlé ». Le prix est une latence égale à la durée du
  bip, 80 ms par défaut, qu'on paie volontiers puisque personne ne commence à parler dans les
  80 ms qui suivent son propre appui sur une touche. Le bip de fin, lui, vient après l'arrêt du
  micro, donc sans risque, et sonne **même sur un appui trop bref** : le bip de début ayant déjà
  retenti, ne pas le refermer laisserait croire que Dictum écoute encore.

  ⚠️ **Un fondu de 5 ms aux deux bouts**, sans quoi la sinusoïde démarre et s'arrête net : la
  discontinuité est un front raide, donc un bruit large bande, qui ne s'entend pas comme un bip
  plus court mais comme un défaut de matériel. Prouvé par mutation, comme le fait que la fréquence
  réglée soit bien celle qui sonne.

  ⚠️ **Un bip muet n'empêche jamais de dicter** : l'échec est noté et la dictée continue. Un
  confort ne doit pas pouvoir bloquer la fonction principale.

- **Un garde-fou contre les réglages qui ne font rien** (`tests/reglages_utilises.rs`).
  ⛔ **Un réglage affiché qui ne pilote rien est un mensonge à l'utilisateur**, et c'est le défaut
  le plus silencieux du produit : l'écran a l'air complet, la valeur se sauvegarde, et rien ne
  change jamais. Le relevé du 2026-09-17 en a trouvé **seize sur vingt-six**, dont deux dans
  l'étape qui venait d'être livrée.
  Il connaît trois catégories, et les distinguer est ce qui le rend utilisable : branché côté
  cœur, **piloté par l'écran seul** (avec sa raison), ou **en attente avec l'étape qui s'en
  chargera**. ⚠️ Il refuse aussi une **exclusion devenue périmée**, sinon la table grossit, décrit
  un passé et finit par couvrir un vrai défaut. Les deux pannes sont prouvées rouges.
  ⚠️ Il lit le cœur **et** l'interface, parce qu'un réglage porte deux noms (identifiant Rust en
  français, clé de fichier en anglais) : ne chercher que le premier accusait à tort tout réglage
  piloté par l'écran, ce qui est arrivé dès le premier essai.

### Fixed
- **Deux réglages de l'étape 7 étaient affichés et ne pilotaient rien.**
  ⛔ La **durée maximale** n'était pas appliquée : une touche coincée enregistrait jusqu'à saturer
  la mémoire, puis envoyait des heures d'audio au moteur. Le plafond est désormais calculé en
  échantillons et appliqué sur le fil audio, un plafond nul valant « pas de limite ».
  ⛔ Le **seuil de silence** existait alors que j'avais codé `f32::EPSILON` en dur. Les deux
  valeurs servent maintenant à distinguer deux situations qu'il ne faut pas confondre : un niveau
  exactement nul veut dire que le microphone n'a **rien** capté, donc une panne qu'on annonce, et
  un niveau faible mais non nul veut dire que **personne n'a parlé**, donc un retour au repos
  silencieux. Les mélanger ferait soit chercher du côté du moteur pour une pièce calme, soit taire
  un micro coupé.
- **La résolution « quel moteur, quel modèle » existait en double.** La copie de la ligne de
  commande ignorait la branche « moteur embarqué » et annonçait « le moteur n'est pas installé »
  sur un moteur livré avec le produit. `dictee::resoudre` est désormais le seul chemin, et rend
  une cause **typée** que chaque appelant traduit : l'interface en français, la ligne de commande
  en anglais. Une chaîne construite au fond aurait été fausse pour l'un des deux.
- **Le raccourci global n'aurait jamais été enregistré.** Les réglages écrivent `Ctrl+Alt+Space`,
  que le greffon ne comprend pas ; et ma traduction renvoyait la chaîne déjà passée en minuscules,
  donc `Space` devenait `space`. Attrapé par son test avant toute exécution.

### Added
- **Le moteur Vulkan existe, et c'est nous qui le construisons.** whisper.cpp ne publie aucun
  artefact Vulkan, donc l'option « une carte graphique AMD ou Intel » etait annoncee indisponible.
  Elle est desormais **la troisieme option installable**, renommee **« N'importe quelle carte
  graphique »** : la mesure montre qu'elle marche aussi bien sur NVIDIA.
  Mesures du 2026-09-17, modele `medium`, meme audio, sur une GTX 1060 :

  | Calcul | Taille | Mediane | Premier appel |
  |---|---|---|---|
  | Processeur (embarque) | 10 Mo | 17,8 s | 16,9 s |
  | CUDA 12.4 | 675 Mo | 3,77 s | 5,5 s (cache JIT deja chaud) |
  | Vulkan (le notre) | **21 Mo** | 4,44 s | **4,43 s** |

  CUDA garde 15 % d'avance, mais pour **trente fois** le poids, une obligation de redistribuer des
  binaires NVIDIA proprietaires, et une penalite de premier appel de 16,9 s a cache froid que
  Vulkan n'a pas du tout.

  Nouveau script `scripts/preparer-moteur-vulkan.py`, qui rejoue toute la recette. **Deux options
  de compilation font la difference entre un artefact distribuable et un artefact qui ne marche
  que sur la machine qui l'a produit**, et le defaut de CMake est le mauvais choix pour les deux :
  - `GGML_NATIVE=OFF` : sans ca le binaire est taille pour le processeur de la machine de
    construction et **plante en instruction illegale ailleurs**. Invisible depuis cette machine.
  - `GGML_BACKEND_DL=ON` : sans ca `ggml.dll` importe `vulkan-1.dll` **statiquement**, et le
    moteur **refuse de demarrer** sur une machine sans pilote Vulkan au lieu de retomber sur le
    processeur. Verifie en retirant le backend : la transcription passe, code de sortie 0.

  Artefact publie en **copie versionnee** sur `dl.breizhzion.com/dictum-desktop/moteurs/`, avec un
  `moteurs.json`. **Verifie et non suppose** : retelecharge depuis l'URL publique et recompare,
  identique bit a bit.

- **Deux scripts pour piloter l'application pendant les tests**, `scripts/capturer-fenetre.ps1` et
  `scripts/cliquer-fenetre.ps1`. Ils portent chacun un verrou, tous deux **declenches pour de vrai
  le jour de leur ecriture** : la capture passe par `PrintWindow` et non par la zone d'ecran
  correspondante, parce qu'une fenetre masquee garde ses coordonnees et qu'on photographie alors
  l'ecran de quelqu'un (deja arrive deux fois) ; et le clic refuse de partir si une autre fenetre
  est au-dessus du point vise, ce qui a evite un clic a l'aveugle sur une session **verrouillee**.

- **CUDA n'est propose que si le pilote NVIDIA est la** (arbitre par painteau le 2026-09-17).
  L'ecran n'affiche plus un telechargement de 675 Mo a quelqu'un dont la machine ne peut rien en
  faire. ⚠️ **On teste la bibliotheque du pilote (`nvcuda.dll`) et pas le nom de la carte** : une
  carte peut porter la bonne marque sans que le pilote qui la rend calculable soit installe, et
  c'est justement ce cas qui produirait un choix telecharge puis silencieusement retombe sur le
  processeur. ⛔ **Sauf si c'est le choix courant**, sinon un reglage recopie d'une autre machine
  donnerait un ecran ou aucune option n'est marquee « utilisé ». `choisir_moteur` refuse aussi,
  la commande etant appelable en dehors de l'ecran.

  ⚠️ **Asymetrie deliberee, Vulkan reste propose partout** alors que `vulkan-1.dll` le conditionne
  tout autant : masquer CUDA ne prive de rien puisque Vulkan reste, tandis que masquer Vulkan sur
  une machine dont le pilote est a mettre a jour retirerait la seule option acceleree. Le cout
  d'une proposition de trop est un telechargement de 21 Mo, pas un mensonge, la sonde disant la
  verite ensuite. **Cas reellement observe** sur la VM Windows : `vulkan-1.dll` present, aucun
  peripherique Vulkan, repli processeur propre.

- **Le runtime Visual C++ part avec l'installateur** (`scripts/preparer-runtime-vcpp.py`, 1,0 Mo).
  ⛔ **Le defaut n'est visible sur aucune machine de developpement** : `dictum.exe` importe
  `vcruntime140.dll`, tout moteur whisper.cpp y ajoute `msvcp140.dll` et `vcomp140.dll`, **Windows
  n'en fournit aucune** et **aucune archive amont ne les embarque**. Sur un Windows neuf, le
  symptome serait une boite de dialogue nommant une DLL.
  Depose a cote de `dictum.exe` plutot qu'installe dans le systeme, parce que notre installateur
  s'execute **sans elevation** (`installMode: currentUser`). Les moteurs les trouvent par le
  `PATH` du processus enfant, Windows cherchant dans le repertoire du binaire LANCE et pas dans le
  notre. Une reponse au niveau de l'application plutot qu'un CRT statique dans nos moteurs :
  celui-ci ne couvrirait pas **le moteur CUDA**, qui vient de ggml-org avec la meme dependance.

### Fixed
- **La sonde d'acceleration annoncait le meme dos d'execution deux fois.** `dedup()` ne retire que
  les doublons **consecutifs** et ne compare pas la casse ; or notre artefact Vulkan emet les deux
  formes d'annonce a la fois (`ggml_vulkan:` et `load_backend: loaded Vulkan`), ce qu'aucun
  artefact amont ne faisait. L'ecran affichait `VULKAN, Vulkan, CPU`. Le cas n'etait couvert par
  aucun test **parce qu'il n'existait pas avant qu'on construise l'artefact soi-meme**.
  Corrige, et prouve rouge par mutation.
- **La construction disait « glob pattern runtime/* path not found » au lieu de ce qu'il faut
  faire.** `src-tauri/moteur/` et `src-tauri/runtime/` sont ignores par git et produits par des
  scripts, donc absents sur une machine fraiche. `tauri_build` echouait deja, mais sur un message
  qui nomme le symptome et ni la cause ni le remede. `build.rs` passe devant et nomme les deux.
  ⛔ **Ne jamais « reparer » ce cas en rendant le glob tolerant** : la construction reussirait et
  l'installateur partirait sans moteur ou sans runtime, defaut qui ne se verrait que chez celui
  qui installe.
- **Un garde-fou d'URL supposait que tout artefact vient d'une publication GitHub.** Il est
  restreint a ce cas, et **complete par sa contrepartie** pour ce qu'on heberge nous-memes : le
  nom de fichier doit porter la version, sinon une republication ecraserait l'empreinte epinglee
  par les installations deja faites, defaut qui ne se verrait qu'a la mise a jour **suivante**.

### Changed
- ⛔ **Le menu de l'icone ne porte que des ACTIONS, jamais un reglage** (arbitre par painteau le
  2026-09-17). Il est reduit a **« Ouvrir Dictum » / « Quitter »** ; les actions utiles
  (enregistrer, traduire) l'y rejoindront a leur etape, pas avant, une entree grisee pour une
  fonctionnalite absente n'apportant rien. **« Demarrer avec le systeme » part dans l'ecran de
  reglages**, qui devient sa seule adresse.
  Le motif n'est pas esthetique : un reglage present a deux endroits finit par y etre affiche
  differemment, ce qui reintroduirait par une autre porte le defaut que « relire l'etat depuis le
  systeme » evitait deja.
  Nouveau module `reglages.rs` (commandes `demarrage_automatique` et
  `definir_demarrage_automatique`), section **Reglages** dans la fenetre avec une **vraie case a
  cocher** et non un faux interrupteur en `div` : clavier, role annonce et etat coche viennent
  gratuitement. **L'invariant est conserve** : la case affiche ce que le systeme dit, jamais ce
  qu'on a demande, donc on relit apres chaque bascule **y compris quand l'ecriture echoue**, et un
  refus s'affiche au lieu de passer en silence.
  **Verifie en cliquant reellement la case** : cle `Run` absente, puis
  `...\Dictum\dictum.exe --autostart` apres avoir coche, puis retiree apres avoir decoche.

- **Le titre de la premiere section ne porte plus de numero d'etape.** Il annoncait « Etape 1 »
  et est reste faux deux etapes durant sans que rien ne le signale. Une interface ne doit pas
  afficher un etat d'avancement que personne ne pense a mettre a jour.

- ⛔ **La surface de ligne de commande passe en ANGLAIS, l'interface graphique reste en francais.**
  L'ancien `--au-demarrage` devient **`--autostart`**, et la sortie de `--help` comme les messages d'erreur
  sont traduits. Motif : l'anglais sera la langue par defaut du produit, et la ligne de commande
  est la premiere surface qu'un public non francophone rencontre.
  ⚠️ **Fait maintenant parce que ce sera une rupture SILENCIEUSE plus tard** : le drapeau du
  demarrage automatique est inscrit par le systeme dans la cle `Run`, donc un ancien nom encore
  present ferait sortir Dictum en erreur **a chaque ouverture de session sans que personne ne voie
  le message**, et l'option paraitrait simplement ne plus marcher. Rien n'etant distribue, il n'y
  a aucun alias a garder ; apres la premiere version publiee, il en faudra un.
  Le test `les_drapeaux_sont_en_anglais` garde la regle dans les deux sens (les noms anglais sont
  reconnus, les anciens noms francais ne le sont plus), **prouve rouge par mutation**. Convention
  ecrite dans le `README.md`, avec la reserve que **les identifiants du code restent en francais**,
  question distincte et non tranchee.

### Added
- **Prechauffage de l'acceleration a l'installation, pour que la premiere dictee ne le paie pas.**
  ⛔ **Mesure : sur une GTX 1060, la premiere transcription apres installation coute 16,9 s, les
  suivantes 1,7 s.** CUDA compile ses noyaux pour la carte au premier usage et les garde dans son
  propre cache. Le cout est paye une fois par machine, mais sans rien faire il tombe sur **la
  premiere dictee**, celle qui donne son impression du produit.
  Dictum fait donc tourner le moteur sur un **WAV de silence de 0,5 s** juste apres l'installation
  du dos d'execution, pendant que l'utilisateur attend deja la fin d'un telechargement de plusieurs
  centaines de mega-octets. Le contenu de l'audio n'a aucune importance : whisper encode la fenetre
  quoi qu'elle contienne, donc du silence exerce exactement les memes noyaux.
  **Prouve en vidant le cache de noyaux de CUDA**, ce qui reproduit une machine neuve : sans
  prechauffage 16,9 s puis 1,7 s ; avec, le prechauffage absorbe 16,3 s et **la premiere vraie
  dictee tombe a 1,6 s**.
  ⚠️ Sans modele telecharge il n'y a rien a faire tourner : la commande renvoie simplement `false`
  au lieu de se plaindre. **Un prechauffage est une optimisation, jamais une condition.**

- ⛔ **Le moteur processeur est desormais EMBARQUE dans l'installateur**, il ne se telecharge plus.
  Dictum dicte **des la fin de l'installation, sans reseau** : preuve faite en vidant tout ce qui
  avait ete telecharge, en installant, puis en lancant `dictum jfk.wav` qui rend son texte sans
  qu'un seul octet soit descendu.

  Le motif n'est pas le confort, c'est que **la promesse du produit est une dictee entierement
  locale** : une application qui doit appeler GitHub pour fonctionner *du tout* se contredit. Et
  toute la surface d'echec du premier lancement (reseau, empreinte, extraction, « moteur non
  installe ») quitte le chemin normal. Les dos d'execution acceleres restent optionnels, donc leur
  echec est rattrapable puisque le processeur marche deja.

  ⚠️ **Jeu minimal MESURE sur les tables d'import PE**, pas devine : `whisper-cli.exe` depend de
  `whisper.dll` et `ggml.dll`, qui dependent de `ggml-base.dll`. Les neuf `ggml-cpu-*.dll`
  n'apparaissent dans **aucune** table d'import parce qu'elles sont chargees **a l'execution**
  selon le processeur : il faut donc les prendre toutes, sous peine d'un moteur qui marche sur la
  machine de developpement et echoue ailleurs. 20,8 Mo d'archive deviennent **10 Mo**, et SDL2
  disparait, soit une licence de moins a porter.
  ⚠️ **Le contenu de `src-tauri/moteur/` n'est pas versionne** : des binaires d'un tiers dans le
  depot se perimeraient en silence. `scripts/preparer-moteur-embarque.py` fait autorite, prouve
  sur ses trois etats (absent, prepare, rejoue).
  ⚠️ **L'avis de licence MIT est RECUPERE a la source** et jamais reconstitue : un avis legal
  approximatif ne remplit pas la condition qu'il pretend remplir.
  ⚠️ **`vcomp140.dll` n'est pas dans l'archive amont** et n'est pas garanti sur une machine neuve.
  Sans lui le moteur ne demarre pas, et Windows ne le nomme pas : l'ecran le dit a sa place.

- **Etape 6 : le moteur de transcription, par la LIGNE DE COMMANDE d'abord.**
  `dictum fichier.wav` rend du texte. Options `-m/--model`, `-l/--language`, `-o/--output`,
  `-q/--quiet`.

  ⚠️ **L'ordre est delibere : la ligne de commande avant l'interface de dictee.** Elle se teste
  sans micro, sans raccourci global et sans injection de frappe, donc elle valide le moteur avant
  qu'on y ajoute trois difficultes d'un coup.
  ⛔ **Le support du processeur graphique n'est pas une question de code Rust, c'est une question
  de QUEL BINAIRE on installe** : whisper.cpp publie un artefact par dos d'execution. Le moteur se
  telecharge donc comme un modele, verification d'empreinte comprise, au lieu d'etre compile avec
  nous. Changer de dos d'execution devient un changement de fichier.
  ⚠️ **Le texte est lu sur la sortie STANDARD, les diagnostics sur la sortie d'erreur.** Le moteur
  ecrit `load_backend:` et `read_audio_data:` sur stderr : les melanger collerait ces lignes dans
  le texte dicte, qui serait ensuite tape dans l'application de l'utilisateur.
  ⚠️ **On extrait TOUT le contenu de l'archive, sans trier** : whisper.cpp livre une dizaine de
  bibliotheques de calcul dont il choisit la bonne a l'execution selon le processeur. En garder une
  sous-selection donnerait un moteur qui marche sur la machine de developpement et echoue ailleurs.
  ⛔ **Chaque chemin de l'archive est verifie avant ecriture** : une archive peut porter des
  chemins qui sortent du repertoire cible. L'empreinte prouve l'origine, pas l'innocuite.

  **Prouve sur la machine** : moteur installe par l'application, puis
  `dictum jfk.wav -m small -l en` rend *« And so my fellow Americans, ask not what your country can
  do for you... »* en 7,6 s. Les cinq voies d'echec ont ete jouees, chacune avec son message et son
  code de sortie : fichier absent, modele inconnu, modele non telecharge, moteur non installe,
  option sans valeur.

- **`chemins.rs` : une seule implementation des emplacements, pour l'interface ET la ligne de
  commande.** Cette derniere n'a pas d'application Tauri sous la main, donc elle ne peut pas
  appeler `app.path()`. ⛔ Si chacune calculait son chemin de son cote,
  `dictum fichier.wav` chercherait les modeles ailleurs que la ou l'interface les a telecharges, et
  dirait « modele absent » sur une machine ou 3 Go de modeles sont parfaitement installes.
  ⚠️ **Configuration et donnees restent dans deux profils differents sur Windows** : les reglages
  en ITINERANT (ils suivent l'utilisateur d'un poste a l'autre, c'est leur role), les modeles en
  LOCAL (3 Go recopies sur le reseau a chaque ouverture de session seraient redhibitoires). Un test
  garde la distinction, et un autre compare l'identifiant recopie a celui du manifeste.

- **Etape 5 : telechargement et verification des modeles.** Catalogue de trois modeles whisper.cpp
  (Small, Medium, Large v3), telechargement avec **reprise**, barre de progression, arret, et
  verification d'integrite par **SHA-256**.

  ⛔ **Present et verifie ne sont pas la meme chose, et l'ecran ne les confond jamais.** Un modele
  tronque par une coupure reseau porte un nom parfaitement normal et peut faire une taille
  plausible : s'il s'affichait « pret », la panne se manifesterait bien plus tard, a la premiere
  dictee, par un message du moteur qui ne parlerait pas de telechargement. Trois etats distincts,
  **absent**, **telecharge non verifie**, **verifie**, ecrits en toutes lettres et pas seulement
  colores.
  ⚠️ **Tailles et empreintes RELEVEES sur la source** (API HuggingFace, l'identifiant LFS d'un
  fichier etant son SHA-256), jamais estimees : une empreinte inventee rendrait ce module pire
  qu'inutile, il refuserait des fichiers sains en accusant le reseau.
  ⚠️ **Le fichier ne prend son nom definitif qu'APRES verification de son empreinte** ; tant qu'il
  descend il s'appelle `.partiel`. Consequence voulue : un fichier portant le nom d'un modele a
  forcement ete verifie.
  ⚠️ **L'empreinte n'est pas recalculee a chaque ouverture** (lire 3 Go pour afficher un ecran
  serait inacceptable, defaut deja rencontre sur l'app iOS) : une marque `.verifie` note le
  resultat, et un bouton rejoue la verification a la demande.
  ⚠️ **Reprise** par requete `Range`, avec un controle que le serveur a bien repondu **206** : sur
  un 200 il renvoie tout depuis le debut, et concatener donnerait un fichier plus gros que prevu
  et illisible.
  ⛔ **Les modeles vont dans `app_local_data_dir` et surtout pas `app_data_dir`** : sur Windows le
  second est dans le profil **itinerant**, donc ces 3 Go seraient recopies sur le reseau a chaque
  ouverture de session. Invisible sur une machine personnelle, et l'application deviendrait
  inutilisable en entreprise.
  ⚠️ **La liste des modeles de l'ecran de reglages est derivee du catalogue** et non recopiee :
  une liste tenue a deux endroits finit par proposer un modele que rien ne sait telecharger, ce
  que faisait l'ancienne version en offrant Parakeet, absent du bureau.

  **Prouve sur la machine, dans les deux sens.** Les trois modeles ont ete telecharges reellement
  par painteau et verifies. Puis **un seul octet sur 487 601 967 a ete retourne a la main**, a
  taille identique donc invisible a tout controle de taille : la verification l'a detecte et l'a
  dit (`Attendu 1be3a9b20638..., obtenu d95a0fec2106...`). Octet restaure, verification a nouveau
  conforme, controlee par `Get-FileHash` qui est un outil independant du notre. Les reglages du contrat de
  fonctionnalites sont la, groupes en Capture / Transcription / Sortie / Interface / Maintenance,
  ecrits dans `reglages.json`. Ce qui demande d'enumerer le systeme (microphones, modeles
  reellement telecharges) arrive a l'etape qui sait le faire.

  ⛔ **Le fichier ne vit PAS dans le repertoire d'installation**, contrairement a l'ancienne
  version : il est dans le repertoire de configuration de l'utilisateur. La desinstallation
  emportait sinon les reglages, et c'est ce choix qui a produit la collision du 2026-09-17.
  ⚠️ **Le contenu du fichier est une entree NON FIABLE** (il s'edite a la main) : chaque valeur est
  ramenee dans ses bornes a la lecture **et** a l'ecriture, une valeur hors liste revient au
  defaut, et une duree minimale superieure a la maximale est corrigee plutot que laissee — elle
  rendrait toute dictee impossible sans qu'aucun ecran ne l'explique.
  ⚠️ **Ecriture en deux temps** (fichier temporaire puis renommage atomique) : une coupure en
  plein `write` laisserait un JSON tronque, donc une configuration perdue.
  ⚠️ **Un fichier illisible ne bloque jamais le demarrage** ; il est conserve en `.invalide`
  plutot qu'ecrase, parce qu'il porte les reglages de quelqu'un et qu'il est la seule piece a
  conviction si le defaut vient de nous.
  ⚠️ **Les cles du fichier sont en anglais** (le format se documente et se partage dans un rapport
  de bug), les libelles en francais. Un test echoue si un identifiant francais fuit dans le format.
  ⚠️ **`#[serde(default)]` au niveau du conteneur** : un fichier ecrit par une version anterieure
  se lit sans erreur, donc ajouter un reglage n'efface la configuration de personne.

  **Prouve en s'en servant** : valeur modifiee a l'ecran, application relancee, valeur tenue ;
  fichier edite a la main, relu et respecte.

- **Reglages simples et avances, separes par une bascule minimale en haut a droite.** Choix
  arbitre contre des sous-ecrans : ceux-ci obligeraient a deviner sous quel onglet vit tel
  reglage, et sur une fenetre de 560 px ouverte depuis la barre, un niveau de navigation
  supplementaire coute plus qu'il ne rend. Les sous-ecrans viendront pour des **fonctions**
  differentes (Modeles, Historique, Telephone), pas pour couper une liste en deux.
  ⛔ **Cacher un reglage modifie le rendrait introuvable** : l'application se comporterait
  autrement sans qu'aucun ecran ne l'explique. Quand les avances sont replies, une note dit
  **combien d'entre eux ne sont plus a leur valeur par defaut**. Les defauts viennent du coeur par
  une commande dediee, **jamais recopies cote interface**, deux listes finissant par diverger.

- **Chaque reglage porte une explication courte, ecrite pour quelqu'un qui decouvre le logiciel.**
  Les 24 en ont une, verifie par un compte et pas a l'oeil. Trois libelles qui etaient du
  vocabulaire de metier ont ete rebaptises : « Fils d'execution » devient **« Coeurs utilises »**,
  « Temperature » devient **« Marge d'interpretation »**, et « Injection » dans le pied de page
  devient **« Ecriture du texte »**.
  ⚠️ Le motif n'est pas cosmetique : un reglage dont on ne comprend pas le nom **ne se touche
  pas**, donc il n'existe pas. Et celui qu'on touche sans comprendre degrade le produit sans qu'on
  sache pourquoi, ce qui coute plus cher que de ne pas l'avoir propose.

- **Les reglages a deux etats sont des interrupteurs a glissiere**, pas des cases a cocher, et
  **tous** le sont : un seul reste en case a cocher et l'ecran hesiterait entre deux affordances
  pour le meme geste. ⚠️ **C'est une vraie `input[type=checkbox]`, seulement redessinee.** Un faux
  interrupteur en `div` avec `role="switch"` obligerait a reconstruire a la main le clavier,
  l'etat annonce, la liaison a l'etiquette et le focus, soit quatre occasions de se tromper en
  silence. ⚠️ **L'etat est porte par la POSITION du curseur autant que par la couleur** : un
  interrupteur qui ne changerait que de teinte tomberait sous le coup de WCAG 1.4.1. Rail de
  40x24 px, donc la cible satisfait le critere 2.5.8 sans meme compter l'etiquette liee, et
  l'animation se coupe sous `prefers-reduced-motion`.

- **Pied de page** : version, plateforme, strategie d'injection, chemin du fichier de reglages, et
  le copyright. ⚠️ **L'annee est calculee a l'execution**, jamais ecrite en dur (convention du
  parc) : une annee figee devient fausse le 1er janvier et personne ne pense a la corriger.
  ⚠️ Le pied est **dans la zone qui defile** et pas colle en bas : un pied fixe mangerait 80 px
  sur une fenetre de 640.

- **Barre de defilement aux couleurs de l'application.** ⚠️ Faite avec `::-webkit-scrollbar` et
  **pas** avec `scrollbar-width`/`scrollbar-color` : sur Chromium, poser l'une des proprietes
  standard fait **ignorer** les pseudo-elements, donc melanger les deux rend la barre par defaut
  sans aucun message.

- **Etape 3 : installateur Windows, demarrage automatique, desinstalleur.** Paquets NSIS et MSI
  en francais, editeur `BREIZHZION`, `installMode: currentUser` (ruche `HKCU`, aucune elevation).
  La version est patchee a la volee dans `tauri.conf.json` par `scripts/version-depuis-tag.mjs`,
  jamais commitee. Option **« Demarrer avec le systeme »** dans le menu de l'icone.
  ⚠️ **L'etat coche est relu depuis le SYSTEME apres chaque bascule**, jamais depuis une
  preference gardee de notre cote : l'ecriture peut echouer (strategie de groupe, antivirus) ou
  l'utilisateur peut retirer l'entree par le gestionnaire de taches, et une case qui resterait
  cochee mentirait alors.
  ⚠️ **Au demarrage de session, l'application s'ouvre dans la zone de notification SANS sa
  fenetre** : le plugin inscrit la commande avec `--autostart`. Ouvrir une fenetre a chaque
  ouverture de session serait la meilleure facon de faire desactiver l'option.

  **Prouve de bout en bout le 2026-09-17**, en pilotant reellement le menu de l'icone et pas en
  relisant le code : cle `Run` absente au depart, puis
  `...\Dictum\dictum.exe --autostart` apres avoir coche, puis retiree apres avoir decoche ;
  lancement avec l'argument, fenetre `IsWindowVisible = False` ; desinstallation laissant registre,
  menu Demarrer et disque nets.
  ⚠️ **Deux indicateurs ont d'abord fait croire a tort que la fenetre s'affichait** :
  `Process.MainWindowHandle` est non nul alors qu'il designe `Tao Thread Event Target`, une
  fenetre technique invisible, et l'`IsOffscreen` de l'automatisation d'interface repondait
  `False` sur une fenetre pourtant masquee. **Seul `IsWindowVisible` sur le bon `hwnd` tranche.**
  ⚠️ **Le repertoire d'installation etait deja celui des donnees de l'ancien Dictum 1.7.1**
  (`%LOCALAPPDATA%\Dictum`, 4425 Mo de modeles) : collision qu'aucune relecture ne pouvait voir,
  et le desinstalleur n'a heureusement retire que ses propres fichiers.

- **Etape 2 : l'icone de la zone de notification**, avec ses quatre etats (repos, ecoute,
  enregistrement, transcription), son menu au clic droit et l'ouverture de la fenetre au clic
  gauche. Fermer la fenetre la **masque** ; seul « Quitter » arrete l'application.
  ⚠️ **La couleur ne porte jamais l'etat seule** : une icone de barre ne peut pas afficher de
  texte, donc c'est l'**infobulle** qui le nomme, et elle change dans la meme fonction que l'image
  pour que les deux ne puissent pas diverger.
  ⛔ **`android-icon-monochrome.png` de l'app iOS n'est PAS la marque Dictum.** Le nom le laisse
  croire, je m'y suis fie, et c'est un **chevron**. Le defaut n'est apparu qu'en **regardant**
  l'image agrandie. La marque est donc **redessinee**, aux proportions **mesurees** sur l'icone
  d'application (anneau 278/235, disque 129 sur un canevas de 1024) et pas estimees.
  ⚠️ Teintes **eclaircies** par rapport a la palette : une barre des taches peut etre claire ou
  sombre, et `preparation` (#7a5218) y disparaitrait sur fond sombre.

- **Etape 1 : la fenetre, sans barre de titre.** Interface en **Tauri** (frontend web, coeur
  Rust), retenu contre egui parce que c'est ce que font deja BeamMeUp et hae-app, que le rendu y
  est entierement maitrise en CSS, et que le parc a deja documente ses pieges.
  Bandeau qui fait office de barre de titre, temoin d'etat repris du logo, commandes redessinees.
  ⚠️ Fermer **masque** la fenetre au lieu de quitter : l'application vivra dans la zone de
  notification, et quitter couperait le raccourci global alors que l'icone serait toujours la.
  ⚠️ Une zone de glissement **avale les clics de ses enfants** : tout bouton pose dessus porte
  `data-non-glisser`, sinon il ne repond jamais et rien ne le signale.
  ⚠️ Supprimer la barre de titre supprime aussi ce qu'elle offrait gratuitement : les commandes
  sont de vrais boutons avec `aria-label` et focus visible, dessines a 32 px pour une cible d'au
  moins 24 (WCAG 2.2, critere 2.5.8).
- **Direction visuelle posee AVANT toute fonctionnalite** (`docs/direction-visuelle.md`) : face
  avant d'appareil de mesure, Fraunces pour les titres (continuite avec l'app iOS), IBM Plex Sans
  et Mono pour le reste, palette imposee non retouchee.
  ⚠️ **Fontes EMBARQUEES, jamais chargees depuis un CDN** : une application de dictee locale doit
  s'ouvrir identique sans reseau, et aller chercher une fonte en ligne contredirait la promesse du
  produit. Les fichiers viennent du depot CDN du parc, donc memes dessins que le site et le mobile.
- **Icone de bureau faconnee depuis le dessin iOS** (`scripts/preparer-icone.py`, idempotent).
  ⛔ Une icone iOS est un **carre pleine page** : c'est le systeme qui l'arrondit. macOS ne le fait
  pas, donc reprise telle quelle elle s'afficherait carree et sans marge dans le Dock. Le script
  applique la grille Apple (824 sur 1024, rayon 185,4) et **verifie le produit** (coins
  transparents, centre opaque) plutot que son intention.
  ⚠️ A ne pas confondre avec l'icone de la zone de notification, qui doit au contraire etre la
  silhouette **transparente** (`android-icon-monochrome.png`, mesuree a 92,5 % de pixels
  transparents contre 0 % pour l'icone d'application). Elle arrive a l'etape 2.

### Fixed
- ⛔ **La sonde d'acceleration ne connaissait qu'un seul format d'annonce, donc elle MENTAIT sur
  Vulkan.** whisper.cpp a change de format entre ses versions : `b5130` ecrit
  `load_backend: loaded CUDA backend from ...`, tandis qu'une version Vulkan de la branche 1.8
  ecrit `ggml_vulkan: Found 1 Vulkan devices:` et **aucun `load_backend` du tout**. La sonde
  n'aurait vu aucun dos d'execution et aurait affiche « ne demarre pas » sur un moteur Vulkan
  parfaitement fonctionnel.
  ⚠️ **Un garde-fou qui ment fait plus de degats que pas de garde-fou du tout** : ici il aurait
  fait renoncer a Vulkan, c'est-a-dire a la seule voie GPU propre en licence. Trouve en mesurant,
  pas en relisant.
  L'analyse du texte est sortie dans une fonction **pure et testable sans lancer de processus**,
  elle reconnait les deux formes, et le test est **prouve rouge par mutation**.

- ⛔ **L'archive CUDA 11.8 de whisper.cpp est CASSEE, et elle echouait en silence.** Elle livre un
  `ggml-cuda.dll` de 518 Mo qui importe `cublas64_11.dll`, **absent de l'archive et absent de
  Windows** : le dos CUDA ne se chargeait jamais, whisper.cpp retombait sur le processeur sans un
  mot, et l'ecran annoncait « carte graphique NVIDIA » sur un calcul fait par le processeur. Le
  seul symptome etait un temps de transcription identique, 7,3 s contre 7,6 s.
  Remplacee par l'archive **12.4**, qui embarque bien `cublas64_12.dll`.
  ⛔ **Le vrai correctif n'est pas le changement d'archive, c'est la SONDE** : Dictum lance
  maintenant le moteur et **lit les dos d'execution reellement charges** dans sa sortie, puis
  l'affiche. Mesure, jamais declaration. Une archive peut passer sa verification d'empreinte, etre
  complete au sens des fichiers presents, et rester incapable de charger son acceleration.

- ⛔ **Changer d'archive pour un meme choix ne reinstallait RIEN.** Le controle etait
  « l'executable existe », pas « c'est la bonne version » : passer de CUDA 11.8 a 12.4 a bascule le
  reglage sans telecharger un octet. Une marque porte desormais l'empreinte de l'archive qui a
  produit l'installation, et le repertoire cible est **vide avant extraction** pour que deux
  versions de bibliotheques ne cohabitent jamais.

- ⛔ **L'ecran des reglages ecrasait le choix du calcul.** Il gardait l'instantane charge au
  demarrage : choisir « carte graphique » puis toucher n'importe quel reglage remettait le calcul
  sur le processeur, sans un message. Il relit le fichier avant d'ecrire et ne remplace que ce
  qu'il pilote.

- ⛔ **La ligne de commande recalculait le chemin du moteur de son cote**, donc elle ignorait la
  branche « embarque » et repondait « moteur non installe » sur une machine ou il etait livre avec
  l'application. ⚠️ C'est exactement la duplication que `chemins.rs` avait ete cree pour supprimer,
  recommise a l'endroit d'a cote. `moteur::executable` ne prend plus d'`AppHandle` : une seule
  fonction, appelee par les deux.

- ⛔ **« Moteur » et « dos d'execution » etaient du vocabulaire de metier a l'ecran**, signale par
  painteau qui a simplement demande ce que le mot voulait dire. L'ecran nomme desormais les deux
  pieces par ce qu'elles font : un **programme de transcription** qui fait le calcul, et des
  **modeles de langue**. « Dos d'execution » etait la traduction litterale de *backend*, un mot que
  personne ne dit et qui n'explique rien.
  ⚠️ **La faute venait d'etre corrigee ailleurs le meme jour** (trois libelles rebaptises dans les
  reglages) et a ete recommise en ajoutant une section : un nettoyage de vocabulaire ne tient pas
  s'il n'est fait qu'une fois. Un controle refuse maintenant que le mot « moteur » reapparaisse
  dans le texte visible de `index.html`, commentaires et identifiants exclus.

- ⛔ **Choisir le modele Small le ramenait a Medium, en silence.** La liste des modeles acceptes
  etait recopiee dans `reglages.rs` et avait deja diverge du catalogue : elle portait `parakeet`,
  qui n'existe pas encore, et ignorait `small`. A la premiere relecture du fichier, `small` etait
  donc juge inconnu et remplace par le defaut, **sans aucun message**. Trouve le 2026-09-17 avant
  d'avoir pu nuire, en branchant la ligne de commande.
  ⚠️ C'est exactement la divergence contre laquelle l'ecran des modeles se premunissait deja en
  derivant sa liste du catalogue : la meme faute avait ete evitee d'un cote et commise de l'autre.
  La validation interroge desormais le catalogue, et un test verifie que **tout** modele du
  catalogue est accepte par les reglages.

- ⛔ **Apres une verification ECHOUEE, la pastille annoncait encore « Verifie ».** Le coeur retire
  bien la marque quand l'empreinte ne correspond pas, mais l'interface ne relisait l'etat que sur
  le chemin de succes : l'ecran affichait donc « Verifie » juste au-dessus du message disant que
  le fichier etait abime. Relecture passee en `finally`.
  ⚠️ **Defaut invisible au typage et aux tests**, vu uniquement sur une capture d'ecran. C'est
  exactement la classe d'erreur que ce module existe pour eviter, reintroduite par la porte de
  l'affichage.

- ⛔ **La liste des modeles s'affichait VIDE une fois sur deux.** Son branchement comptait sur le
  chargement du catalogue fait par l'ecran de reglages, or les deux partent en parallele : selon
  lequel finissait le premier, l'ecran se dessinait avant d'avoir des donnees. Aucun message,
  aucune erreur, juste une section vide. Chaque ecran relit desormais ce dont il a besoin.

- ⛔ **L'attribut `hidden` ne masquait RIEN sur les reglages avances.** Il ne masque que par une
  regle `display: none` de la feuille du navigateur, qui a la specificite la plus faible : notre
  `.reglage { display: grid }` la battait, et les champs restaient **visibles alors que
  l'attribut etait bien pose**. Symptome trompeur, la case decochee ne repliait simplement rien,
  ce qui envoie chercher le defaut dans le JavaScript. Corrige par une regle
  `[hidden] { display: none !important }`, a garder sur tout projet qui pose `hidden` a la main.

- ⛔ **Configuration d'empaquetage restauree apres l'avoir detruite moi-meme.** Un
  `git checkout -- src-tauri/tauri.conf.json`, lance pour annuler le seul patch de version, a
  emporte avec lui **tout le reste du fichier qui n'etait pas commite** : `publisher`, `category`,
  les langues NSIS et `installMode: currentUser`, la langue WiX. Le defaut ne s'est vu qu'a la
  construction suivante, ou le MSI est sorti en `en-US` au lieu de `fr-FR`.
  ⚠️ **Lecon : `git checkout -- <fichier>` est destructeur sur un fichier qui porte du travail non
  commite, et il ne previent pas.** Lire le diff avant de restaurer, ou committer d'abord. Ici le
  reste du repertoire avait bien ete commite dans le meme tour, ce fichier seul a ete rendu a sa
  version d'etape 1.

- ✅ **`unraid-windows` remise en etat** (WinRM), et mon diagnostic precedent etait **imprecis**.
  Je disais « tous les binaires tronques a 0 octet » : en realite les raccourcis de `.cargo/bin`
  sont des **liens symboliques**, et un lien symbolique affiche TOUJOURS une taille de 0. Le seul
  fichier reellement perdu etait **`rustup.exe`, leur cible commune**, ce qui faisait pointer
  chaque lien vers le vide, d'ou le « Aucune application n'est associee au fichier specifie ».
  ⚠️ Reparation en deux temps : `rustup-init` seul ne remplace pas des raccourcis existants, il a
  fallu **supprimer les liens orphelins d'abord**.
  ⚠️ Correction aussi de « la VM n'a pas Git Bash » : elle l'a, a
  `C:\Program Files\Git\bin\bash.exe`, il n'etait simplement **pas dans le PATH**.
  Ajoute **en fin** de PATH machine, car place devant il masquerait `find.exe`, `sort.exe` et
  `more.exe` de Windows par leurs homonymes Unix.
- **La VM dediee passe devant le poste de travail** dans le choix du runner Windows, maintenant
  qu'elle est saine. Corollaire direct de la regle posee plus haut : a capacite egale, le travail
  va sur la machine dediee.
- ⛔ **`unraid-windows` a une installation Rust CORROMPUE** : `cargo.exe` est present mais **ne
  s'execute pas** (« Aucune application n'est associee au fichier specifie »), vraisemblablement
  tronque par l'action de cache retiree. Le workflow nomme donc `office-windows` en priorite et
  **annonce le repli** sur l'autre. **La machine reste a remettre en etat a la main.**
  ⚠️ Et la detection **execute** desormais `cargo --version` au lieu de se contenter de
  `Get-Command` : un fichier du bon nom qui ne demarre pas faisait declarer l'outillage present,
  et l'echec eclatait trois etapes plus loin sur un message sans rapport avec sa cause.
  **Constater qu'un fichier existe ne prouve pas qu'il fonctionne.**
- ⛔ **Les deux runners Windows du parc ne sont PAS interchangeables**, alors qu'ils portent les
  memes etiquettes et se relaient sans prevenir. `office-windows` a Git Bash, **`unraid-windows`
  ne l'a pas** : toute etape en `shell: bash` y rend `bash: command not found`. Un workflow qui
  suppose bash sous Windows marche donc **une fois sur deux, au hasard du runner**, ce qui est
  pire qu'un echec franc.
  Les trois etapes concernees sont dedoublees par systeme (`Chaine Rust`, la verification de
  version au tag, le renommage pour la publication), et un controle a balaye les deux workflows
  pour verifier qu'aucune etape `bash` ne peut plus atterrir sous Windows.
- ⚠️ **L'etape « Chaine Rust » echouait sur `unraid-windows`**, ou `rustup` manque alors que
  `cargo` est present. Son absence n'empeche pas de compiler, elle empeche seulement d'honorer le
  canal epingle par `rust-toolchain.toml` : c'est donc un **avertissement** et plus un echec.
  Bloquer la CI la-dessus la rendrait rouge sur un defaut de provisionnement de machine, pas de
  code, et une CI rouge au hasard finit ignoree.
  ⚠️ **Correction d'une affirmation precedente** : les runs qui ont efface `.cargo/bin` tournaient
  bien sur `office-windows`, donc **sous le compte de painteau**, ce qui confirme la
  responsabilite de l'action de cache. Le run suivant est parti sur `unraid-windows`, une VM sous
  le compte `Admin`, ce qui m'avait fait croire a tort que la CI ne pouvait pas toucher son
  profil. **Toujours verifier QUELLE machine a pris le job avant de conclure** : deux runners
  portent les memes etiquettes et se relaient sans prevenir.
- ⛔ **Aucun bouton de la fenetre ne repondait, et la cause etait le systeme de permissions de
  Tauri 2.** Il n'y avait **aucun repertoire `capabilities/`**, donc zero permission declaree :
  `hide()` et `minimize()` etaient **rejetes**. La commande personnalisee `etat_plateforme`
  fonctionnait, elle, parce que les commandes de l'application ne sont pas soumises au meme
  controle que celles du coeur, ce qui rendait le defaut d'autant plus trompeur.
  ⚠️ **Et `void promesse` avalait le rejet.** Le bouton paraissait inerte, la console restait
  muette. J'ai cherche ailleurs pendant longtemps (zone de glissement, activation de la fenetre,
  coordonnees du clic, premier clic qui active) **precisement parce que l'erreur ne se voyait
  nulle part**. Tout appel au coeur passe desormais par une fonction qui **rapporte** le rejet.
  C'est le defaut que je pourchasse partout ailleurs, commis ici.
- ⚠️ **`no-drag` sur le conteneur ne suffit pas**, la propriete ne se transmet pas aux enfants :
  les boutons poses dans la zone de glissement doivent la porter eux-memes. C'etait le piege que
  mon propre commentaire decrivait deux lignes plus haut.
- ⚠️ **Le menu de l'icone etait construit puis jamais attache** au constructeur : le clic droit
  n'aurait rien affiche. Attrape par **clippy** (`unused variable: menu`) et pas par la relecture,
  qui voyait un menu bien forme quelques lignes plus haut.
- ⚠️ **Le clic gauche depuis le debordement de Windows 11** (le chevron qui replie les icones) ne
  delivre pas la meme sequence qu'un clic sur une icone epinglee : filtrer sur `MouseButtonState::Up`
  rendait le clic totalement inerte. On ne filtre plus sur l'etat du bouton, `montrer_fenetre`
  etant idempotente.
- ⚠️ **La CI Windows echouait en constatant la presence de Rust, pas en l'utilisant.** Invoquer un
  binaire natif dans une interpolation pwsh (`"... $(cargo --version)"`) echoue en session non
  interactive de runner : `StandardOutputEncoding is only supported when standard output is
  redirected`. Constater une presence n'exige pas de lancer le programme : on rapporte desormais
  le **chemin** trouve, et la version s'affiche a l'etape suivante, qui a une sortie redirigee.
- ⛔ **La CI n'installe plus RIEN sur une machine qui n'est pas dediee, et c'est une regle de
  parc, pas un reglage de ce depot.** `office-windows` est le **poste de travail** de painteau,
  `macbook-arm64` est le Mac **d'un collegue, hors infrastructure**. Le 2026-09-16,
  `%USERPROFILE%\.cargo\bin` a ete **vide DEUX FOIS** sur
  le poste du bureau : Rust y est devenu inutilisable, les toolchains de 1,9 Go restant intacts
  mais tous les raccourcis disparus. Repare avec `rustup-init`, mais **casser le poste de
  quelqu'un pour faire passer une CI est un prix qu'on ne paie pas**.
  ⛔ **J'ai d'abord accuse `winget install`, et c'etait FAUX.** Le vrai coupable est
  **`Swatinem/rust-cache@v2`**, dont l'option `cache-bin` (activee par defaut) fait **nettoyer
  `~/.cargo/bin`** a son etape de fin, ce que le journal dit en toutes lettres
  (`... Cleaning cargo/bin ...`). Ce qui a tranche : le **second** effacement est survenu sur une
  execution ou **plus aucune installation ne tournait**. Et la sauvegarde du cache **echouait
  ensuite** (`gzip: command not found`), donc l'action supprimait les binaires **sans meme
  produire le cache** pour lequel elle les supprimait. Elle est retiree des deux workflows : sur
  des runners persistants `~/.cargo/registry` survit d'une execution a l'autre, donc elle
  n'apportait presque rien.
  ⚠️ Lecon de methode : **un premier coupable plausible n'est pas un coupable**.
  macOS et Windows se contentent desormais de **trouver** l'outillage et d'echouer en disant quoi
  faire s'il manque. Linux garde l'installation automatique, ses runners (`fugu`, `ofraid`,
  `monminilab`) etant des machines dediees au role.
  ⚠️ **Le declencheur etait une detection trop naive** : `Get-Command cargo` echouait alors que
  Rust etait installe, la session du runner ayant demarre avant. On teste donc aussi le chemin
  d'installation connu avant de conclure a une absence.
- ⛔ **La CI Windows echouait alors que Rust etait deja installe**, sur `office-windows`.
  Deux defauts cumules.
  ⚠️ **La branche Windows n'ajoutait JAMAIS `.cargo\bin` au `GITHUB_PATH`**, alors que la branche
  Unix le faisait. `Get-Command cargo` concluait donc a tort que Rust etait absent, la session du
  runner ayant demarre avant l'installation de rustup. La detection teste desormais aussi le
  chemin d'installation connu.
  ⛔ Et **le piege pwsh du parc, reproduit a l'identique** (documente dans `docs/distribution.md`) :
  `winget` rend **1** quand le paquet est deja installe et qu'il n'y a rien a mettre a jour, ce qui
  n'est pas un echec, mais le wrapper pwsh de GitHub termine par `exit $LASTEXITCODE` et cette
  variable **survit** au traitement du cas. L'etape echouait donc alors que son travail avait
  reussi. Remise a zero explicite, et **le resultat est juge sur le PRODUIT** (le binaire
  existe-t-il) et jamais sur le code de sortie de winget.
- ⚠️ **La CI Linux echouait sur le verrou `dpkg`.** `DPkg::Lock::Timeout=300` fait desormais
  ATTENDRE apt au lieu d'abandonner. Une CI rouge au hasard finit ignoree, donc ce genre d'echec
  se traite au lieu de se relancer.
  ⛔ **La cause n'etait PAS ce que j'avais suppose.** J'ai d'abord accuse les mises a jour
  automatiques de Debian ; verification faite sur la machine, `unattended-upgrades` n'y est meme
  pas installe. L'historique apt montre que le verrou etait tenu par **notre propre execution
  precedente**. Mecanisme a retenir : un runner self-hosted ne traite qu'un job a la fois, mais
  `cancel-in-progress: true` a annule le premier, GitHub a libere le runner qui a pris le suivant
  **pendant que le `sudo apt-get` du job annule tournait toujours**. Une annulation tue l'etape,
  pas ses processus fils lances sous `sudo`.
- ⚠️ **La CI Linux echouait sur le verrou `dpkg`, pris par un autre `apt-get`** (probablement les
  mises a jour automatiques de Debian). Echec purement environnemental, sans aucun rapport avec le
  code. `DPkg::Lock::Timeout=300` fait desormais ATTENDRE apt au lieu d'abandonner.
  **Une CI rouge au hasard finit ignoree**, donc ce genre d'echec doit etre traite et pas relance.
- Les schemas de permissions Tauri (`src-tauri/gen/schemas/`) ne sont plus versionnes : ils sont
  regeneres a chaque compilation par `tauri-build`, donc les suivre ne produirait que du bruit de
  diff a chaque construction.
- ⛔ **Garde-fou contre le piege `custom-protocol`, paye une fois de plus ici.** Compiler avec
  `cargo build` nu au lieu de `npm run tauri build` n'active pas cette fonctionnalite, et le
  binaire va alors chercher l'interface sur **localhost** au lieu des fichiers embarques : la
  fenetre s'ouvre sur « localhost refused to connect », la ligne de commande fonctionne, l'IPC
  fonctionne, et **rien dans la compilation ne le signale**. Le binaire refuse desormais de
  demarrer dans ce cas, en disant quoi faire.
- ⛔ **Les textes d'interface etaient sans accents** (« ETAPE 1 », « PRET », « La fenetre existe »).
  Les sources avaient ete ecrites en ASCII par reflexe, ce qui est la convention du parc pour les
  **commentaires** mais jamais pour ce que lit l'utilisateur.
  ⚠️ **Defaut invisible a la relecture du code et trouve en REGARDANT la fenetre**, ce qui est
  exactement la raison pour laquelle chaque etape doit finir par un lancement reel.

- **Squelette du projet et chaine de verification, posee avant toute fonctionnalite.** CI qui
  verifie le formatage, passe `clippy` en erreurs et lance les tests **sur les trois systemes**,
  plus une compilation en release. C'est deliberement la premiere chose ecrite : l'ancienne
  version de Dictum a livre sept versions mineures avec 7258 lignes de Rust, **zero test** et
  **aucun workflow qui compile seulement le projet**.
  ⚠️ **Eprouvee sur ses trois modes de defaillance avant d'etre gardee** (assertion fausse,
  avertissement `clippy`, ecart de formatage) : une verification qu'on n'a jamais vue rouge ne
  prouve pas qu'elle s'execute.
- **Module `platform` : detection de la strategie d'injection de texte.** Fonction pure, donc
  testable depuis n'importe quelle machine sans toucher a l'environnement reel.
  ⛔ Il encode le point le plus contre-intuitif du projet : **Omarchy est la cible la plus
  difficile alors que c'est la plus recente.** `xdg-desktop-portal-hyprland` fournit la capture
  d'entrees mais **pas** le portail `RemoteDesktop`, donc la voie `libei` y **n'emet rien du
  tout** ; le repli est `zwp_virtual_keyboard_v1`. Un bureau composite comme `Hyprland:GNOME` doit
  donc etre classe en clavier virtuel et jamais en `libei`, ce qu'un test verifie nommement.
  ⚠️ Un environnement muet rend `Indeterminee` et **n'est jamais devine** : une supposition fausse
  produirait une injection silencieusement inerte, exactement le mode d'echec qu'on supprime.
- **Publication sur tag `vX.Y.Z`**, conformement a la convention du parc : rien ne se publie sur
  un push de branche, un tag ou un declenchement manuel seulement. Construit les trois systemes et
  attache les binaires a une release GitHub.
  ⚠️ `fetch-depth: 0` est obligatoire dans le checkout, sans l'historique complet et les tags
  `git describe` ne voit rien et le binaire s'annoncerait `0.0.0`.
- **La version est DERIVEE du tag git par `build.rs`, jamais declaree.** `Cargo.toml` reste fige a
  `0.0.0`. Hors de tout tag, le binaire annonce le hache court et le suffixe `-dirty` quand
  l'arbre est modifie, donc on sait qu'on ne teste pas ce qui est commite.
  ⚠️ Un numero ecrit a la main a cote du code qu'il decrit finit toujours par mentir. Ici il ne
  PEUT pas diverger, et le workflow de publication **refuse de publier** un binaire qui ne
  s'annonce pas comme le tag qui l'a construit.
- **`--version`, `--help`, et le refus d'un argument inconnu** avec un code de sortie non nul.
  L'interpretation des arguments est une fonction pure, testee sans lancer de processus.
  ⚠️ `--version` n'imprime que le numero, sans rien autour : c'est une exigence et pas une
  preference, la publication compare cette sortie au tag caractere pour caractere.
- **CI et publication sur les runners MAISON du parc**, jamais sur les runners GitHub.
  ⛔ Corrige une faute du premier jet : ce depot est **prive**, donc chaque minute GitHub y est
  **facturee**, et macOS l'est **dix fois**. Le motif du parc est en fait conditionne a la
  visibilite : les depots **publics** (noisecrypt, beammeup, hublot) utilisent les runners GitHub
  parce qu'ils y sont **gratuits**, les depots **prives** (hae-app, dictum-app, dictum-api) passent
  tous en self-hosted.
  Un job `cibles` resout un runner par systeme en interrogeant l'organisation, et **ne se replie
  sur GitHub qu'en le disant** (`::warning::`), plutot que de laisser le travail en file d'attente
  indefiniment si une machine est hors service. Motif repris de `hae-app` et `justmakeq`.
  ⚠️ **Linux ne demande PAS l'etiquette generique `[self-hosted, Linux, X64]`** : cinq machines la
  portent, donc l'attribution serait un tirage au sort incluant `olivmama94-bzhzion`, documente
  « pas de builds lourds » avec ses 3,8 Gio de RAM, ce qu'une compilation Rust en release avec LTO
  est. Le parc a deja paye ce piege : un deploiement a mis **24 minutes au lieu d'une** parce
  qu'une machine lente avait gagne le tirage. Les machines sont donc nommees par ordre de
  preference, `fugu` en tete.
  ⚠️ Rust n'est pas garanti present sur ces machines : installation conditionnelle de rustup, puis
  ajout au `GITHUB_PATH`, **qui ne vaut que pour les etapes suivantes** et jamais pour celle qui
  l'ecrit (piege deja paye ailleurs dans le parc).
- Chaine Rust epinglee par `rust-toolchain.toml`, sans quoi un avertissement `clippy` peut etre
  rouge en CI et vert sur un poste.
