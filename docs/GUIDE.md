# Guide détaillé — Nexus Remote

Pour une présentation du produit et une prise en main rapide, consulter le [README](../README.md). Ce guide rassemble les détails de fonctionnement, de mise à jour et de validation.

Un espace de travail natif en français pour SSH, SFTP et RDP. Cette version reprend les usages locaux de Termius : carnet d’hôtes, groupes, identités, snippets, tunnels et espaces de travail, avec RDP intégré.

Le code applicatif et l’interface sont écrits en Rust (egui/eframe), sans Electron, WebView, JavaScript, C# ou .NET. libssh2, OpenSSL et zlib sont intégrés statiquement. Windows 10/11 x64 et un pilote OpenGL 3.3 sont nécessaires. L’exécutable n’est pas signé.

## Démarrer et mettre à jour

Extraire le ZIP, fermer l’ancienne version et ouvrir `NexusRemote.exe` sans argument pour retrouver le carnet personnel existant. Fermer l’application déconnecte ses vues SSH/RDP ; cela n’arrête pas les machines distantes. Les connexions actives et leurs onglets ne sont pas conservés par ce redémarrage : il faut les rouvrir depuis le carnet. Ajouter un hôte avec son protocole, son adresse, son port et son utilisateur.

Le carnet reste dans `%LOCALAPPDATA%\NexusRemoteRust`. Les anciennes connexions sont conservées. Lors de la première mise à jour depuis la version 0.1, la première écriture garde une copie exacte de cet ancien fichier dans `connections.v0.1-backup.json`. Un fichier invalide est conservé ; une seule instance peut ouvrir le même carnet.

## Onglets SSH et vue côte à côte 0.3.7

Chaque onglet SSH garde sa connexion pendant que vous travaillez dans un autre onglet. Nexus traite aussi la sortie des onglets masqués par portions bornées à chaque rafraîchissement ; une file de sortie pleine ne doit donc plus figer un shell tmux uniquement parce que son onglet est inactif. Le shell tmux reste géré par le serveur Linux : fermer l’onglet Nexus coupe la connexion SSH, mais ne remplace pas les commandes de gestion de tmux.

Double-cliquer sur un onglet, ou ouvrir son menu au clic droit puis choisir **Renommer l’onglet…**, change son titre pour cette session seulement. Le nom de l’hôte dans le carnet ne change pas. Le titre est conservé lors de **Reconnecter**, puis disparaît avec la session lorsque l’application est fermée.

Le bouton **Côte à côte** au-dessus des onglets permet de choisir la session à placer à droite. Le menu au clic droit d’un onglet permet aussi de l’afficher à gauche ou à droite. Cliquer l’onglet du volet droit le garde dans ce volet. Une nouvelle connexion remplace la vue de gauche sans fermer la session de droite ; visiter le coffre conserve la disposition pour le retour aux sessions. Le menu **Session** permet toujours d’empiler les deux vues verticalement ou de revenir à une seule vue.

## Menu contextuel SFTP 0.3.6

Un clic droit sur une ligne de la vue SFTP sélectionne l’élément visé et affiche les actions adaptées au panneau local ou distant. Les commandes du menu utilisent les mêmes contrôles que les boutons de la vue : seuls les fichiers ordinaires sont transférés, les liens symboliques ne sont pas transférés, l’édition distante reste limitée à 1 Mo et la suppression distante demande une confirmation. Aucun dossier n’est transféré de manière récursive ; aucun fichier local n’est supprimé par ce menu.

## Vérification du certificat RDP et tunnels distants 0.3.5

Pour les connexions RDP intégrées et celles ouvertes avec le client Windows, Nexus exige par défaut que Windows authentifie le serveur. Un ancien profil sans réglage de certificat suit aussi cette règle. Dans les paramètres avancés de chaque hôte RDP, **Autoriser un certificat RDP non vérifié** permet explicitement de poursuivre après un avertissement Windows. Cette exception concerne seulement cet hôte ; elle ne vaut pas approbation du certificat. Si la connexion échoue avec la règle stricte, vérifier le certificat et le nom du serveur avant d’envisager l’exception. Ne saisir aucun identifiant dans une session dont l’identité reste douteuse.

Chaque démarrage d’un tunnel SSH distant `-R` montre l’hôte, le port et la destination, puis demande une confirmation. Le client demande une écoute loopback, mais le serveur SSH peut imposer une écoute publique avec `GatewayPorts yes`. Nexus ne reçoit pas l’adresse d’écoute effective dans la réponse SSH et ne peut donc pas garantir que le port est privé. Vérifier `GatewayPorts no` ou `clientspecified` sur le serveur et, si nécessaire, contrôler l’écoute directement sur celui-ci.

## Correctif de saisie SSH 0.3.3

Cette version corrige la reprise du clavier après un passage entre RDP et SSH. Lorsqu’une vue RDP est masquée, le clavier revient à Nexus seulement si l’application est au premier plan et si ce contrôle distant détenait le focus ; les autres contrôles et applications gardent leur focus.

Sélectionner un onglet SSH permet de reprendre directement la saisie, sans clic supplémentaire dans le terminal. Si sa recherche est ouverte, le texte reste destiné au champ de recherche. Les menus, les fenêtres modales et les autres panneaux conservent leurs protections contre l’envoi involontaire de texte au shell.

Le format du carnet, les profils et les identifiants enregistrés restent inchangés. Le redémarrage nécessaire pour charger le correctif déconnecte les sessions ouvertes ; il ne transfère pas leurs onglets ou leur shell vers la nouvelle version.

## Vues de session 0.3.2

Les sessions SSH et RDP disposent d’onglets plus compacts, avec l’icône du protocole et le bouton de fermeture réunis dans chaque onglet. Un seul bandeau par vue rassemble l’adresse, l’état de connexion et les commandes ; les panneaux partagés gardent chacun leur propre bandeau. Le menu **Session** donne accès aux espaces de travail, au partage de vue et, pour SSH, aux snippets et à la reconnexion.

Dans le terminal SSH, la recherche reste à portée de clic. La copie, le zoom, les thèmes, l’export de l’écran et la déconnexion sont regroupés dans les commandes du terminal, avec une présentation adaptée à la largeur du panneau. La recherche apparaît seulement lorsqu’elle est ouverte ; les lignes permanentes d’aide et de statut ont été retirées du corps du terminal pour agrandir sa surface utile.

Dans une vue RDP, les commandes d’ajustement et de plein écran sont complétées par un menu d’options pour la connexion, la déconnexion, le client Windows et les raccourcis. Les messages détaillés apparaissent quand ils sont utiles. La zone native reste contenue dans son panneau et se masque temporairement pendant l’ouverture d’un menu de Nexus, afin que le bureau distant ne recouvre pas ce menu.

La refonte conserve l’authentification Windows, NLA et les avertissements de certificat. Elle ne permet pas, à elle seule, de conclure sur la cause d’un écran RDP noir ni de garantir sa résolution ; la connexion concernée doit être vérifiée séparément.

## Interface et carnet d’hôtes

La palette reprend la référence [GoaCloud UI, carte produit GC-CRD-01](https://design.goacloud.fr/#GC-CRD-01) : fond vert sombre, accent menthe et surfaces arrondies. La navigation, les formulaires et les panneaux des sessions utilisent cette même palette. Les polices Inter et JetBrains Mono sont embarquées : aucune récupération de police sur Internet n’est nécessaire.

Les hôtes utilisent désormais des tuiles plus compactes : icône à gauche, nom et adresse rapprochés, compte visible et commandes en pied. Les boutons ordinaires mesurent 32 px, les badges de protocole 22 px. Les notes restent accessibles dans les détails et les infobulles. La barre de recherche, les filtres, le changement de vue et la grille partagent un budget de largeur qui conserve la marge droite lors d’un redimensionnement. Les alignements du pied de la barre latérale et de la barre d’état sont repris.

Ces ajustements s’appuient aussi sur l’analyse des composants livrés dans une installation locale de Termius 10.4.0. Le contrôle automatique de sa fenêtre étant bloqué, il s’agit d’une analyse de ses ressources d’interface, pas d’une observation de l’écran actif. Aucun code, icône ou police propriétaire de Termius n’est réutilisé.

Cette version conserve les fonctions des versions 0.2, 0.3 et 0.3.1 ainsi que le même format de carnet. Un dossier fourni avec `--data-dir CHEMIN` ouvre un carnet distinct réservé aux essais ; il ne sélectionne pas le carnet personnel habituel.

## Fonctions

- **Hôtes** : grille/liste, recherche, groupes, étiquettes, notes, favoris, récents et tri. Un clic affiche les détails, un double-clic connecte. Le menu contextuel des groupes permet de les configurer.
- **Identités** : réutiliser un utilisateur et une méthode d’authentification dans plusieurs hôtes. Les secrets peuvent rester en mémoire ou être enregistrés avec le chiffrement Windows DPAPI. Les fichiers de clés privées restent à leur emplacement.
- **Connexion rapide** : `Ctrl+K`, puis rechercher un hôte ou saisir `admin@serveur:2222`, `ssh://`, `sftp://` ou `rdp://`. Les mots de passe se saisissent séparément.
- **SSH** : terminal interactif UTF-8/ANSI, couleurs, écran alternatif, historique, sélection, collage multiligne confirmé, recherche, zoom et thèmes Nexus/Dracula/Solarized. Export explicite de l’écran affiché.
- **Vue partagée** : deux sessions côte à côte ou empilées. Les espaces de travail enregistrent une sélection d’hôtes à rouvrir ensemble, sur action explicite.
- **Snippets** : enregistrer, copier et insérer des commandes. L’exécution montre la commande et les sessions SSH sélectionnées avant l’envoi avec Entrée, y compris sur plusieurs sessions.
- **SFTP** : fichiers locaux/distants, filtres, fichiers masqués, navigation, menu au clic droit selon l’élément, envoi/téléchargement, progression et annulation. Création de dossiers distants, renommage et suppression de fichiers distants ou de dossiers distants vides. Éditeur UTF-8 jusqu’à 1 Mo avec sauvegarde confirmée, détection des modifications concurrentes et remplacement protégé.
- **Tunnels SSH** : redirection locale `-L` avec écoute loopback, et redirection distante `-R` avec confirmation à chaque démarrage. Pour `-R`, Nexus demande loopback mais le serveur contrôle l’adresse réellement écoutée : `GatewayPorts yes` peut exposer le port au réseau. Vérifier `GatewayPorts no` ou `clientspecified` sur le serveur avant de démarrer. Flux simultanés et tampons bornés.
- **RDP** : contrôle Windows dans l’onglet, NLA/CredSSP, ajustement, plein écran et presse-papiers configurable. Authentification du certificat obligatoire par défaut, avec exception avancée par hôte permettant de poursuivre après un avertissement Windows. Le client Windows séparé reste accessible sans mot de passe dans le fichier `.rdp`.

## Données et sécurité

Les métadonnées du carnet (adresses, noms, notes, commandes et chemins) restent en clair sur ce PC. Les secrets enregistrés sont chiffrés par DPAPI pour le compte Windows courant. Les exports de hôtes ne contiennent ni secret ni contenu de clé privée ; les identités sont matérialisées sans leur mot de passe.

La première connexion SSH/SFTP présente l’empreinte SHA-256 avant authentification. Comparer cette empreinte avec celle du serveur avant acceptation. Une clé modifiée est refusée ; les clés connues se gèrent dans **Empreintes SSH**.

La compilation de cette version utilise OpenSSL 3.5.9 LTS. Au 19 novembre 2026, il s’agit de la dernière publication de la branche 3.5 répertoriée par OpenSSL. Les [avis du 10 novembre](https://openssl-library.org/news/secadv/20261110.txt) et [du 17 novembre](https://openssl-library.org/news/secadv/20261117.txt) signalent huit problèmes de gravité *Low* affectant cette branche. Leurs corrections figurent dans le dépôt amont ; la publication 3.5.10 qui les contiendra est annoncée pour le 1er décembre 2026. Leur applicabilité précise au transport SSH de Nexus n’a pas été établie. La branche 3.6 précédemment intégrée est [hors support depuis le 1er novembre 2026](https://openssl-library.org/post/2026-11-01-eol34-36/).

L’import JSON et l’import de blocs explicites OpenSSH `Host` sont disponibles. L’import OpenSSH reprend HostName, User, Port et IdentityFile. Include et ProxyCommand ne sont pas exécutés ; une configuration SSH complète n’est pas reproduite.

À l’import JSON, les exceptions de certificat RDP non vérifié et le partage du presse-papiers RDP sont désactivés pour chaque hôte importé. Vous pouvez les réactiver dans les paramètres avancés après vérification.

## Raccourcis

`Ctrl+K` : connexion rapide ; `Ctrl+N` : nouvel hôte ; `Ctrl+F` : recherche du coffre ; `Ctrl+Maj+F` : recherche du terminal ; `Ctrl+Maj+C/V` : copier/coller du terminal ; `Maj+Page précédente/suivante` : historique ; `Ctrl+Alt+Fin` : sécurité Windows dans RDP.

## Périmètre

Nexus Remote n’est pas affilié à Termius. La synchronisation cloud, la collaboration d’équipe, Mosh/Telnet/série, FIDO2, les chaînes de rebond SSH, SOCKS dynamique, les transferts récursifs de dossiers et l’autocomplétion par IA ne sont pas implémentés.

SSH, SFTP et tunnels sont testés avec un vrai serveur localhost et des clés Ed25519 éphémères. Le contrôle RDP natif est testé sans connexion ; une session distante complète reste à valider sur votre infrastructure.

## Compiler

Prérequis sur Windows x64 :

- Rust stable installé avec rustup pour `x86_64-pc-windows-msvc`.
- Visual Studio 2022 Build Tools avec la charge **Développement Desktop en C++**, les outils MSVC x64 et le SDK Windows.
- Perl et `tar.exe` dans `PATH` (par exemple Strawberry Perl et le `tar.exe` de Windows), nécessaires à la compilation d’OpenSSL.

Depuis le dossier du dépôt, ouvrir un **Developer PowerShell for VS 2022** configuré pour x64, puis lancer :

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-openssl.ps1
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Test
```

`build-openssl.ps1` récupère l’archive officielle d’OpenSSL 3.5.9 LTS et vérifie son SHA-256 avant de l’extraire. Il compile OpenSSL pour MSVC x64, lance sa suite de tests et installe la bibliothèque statique sous `.local-build\openssl-3.5.9\install`. Il crée un manifeste d’empreintes pour les bibliothèques installées et refuse de réutiliser des sources extraites lors d’une relance ; utilisez un nouveau `-OutputDir` pour refaire la compilation. Pour un build sans réseau, fournir une archive déjà téléchargée avec `-SourceArchive CHEMIN -Offline`. `build.ps1` vérifie le manifeste et configure Cargo pour la lier statiquement sans revenir à l’ancienne copie OpenSSL 3.6 hors support. Il teste toujours la version OpenSSL liée, puis exécute toute la suite Rust avec `-Test`.

Les scripts n’installent ni ne mettent à jour Rust, Perl ou le SDK. Cargo récupère les dépendances de `Cargo.lock` au premier build ; `-Offline` permet de travailler sans réseau une fois leur cache rempli. Le runtime C est lié statiquement via `.cargo/config.toml`. Pour lancer Cargo directement, définir `OPENSSL_DIR` vers l’installation validée, `OPENSSL_STATIC=1` et `OPENSSL_NO_VENDOR=1` dans le même terminal. Ces variables sont définies automatiquement par `build.ps1`.

Tests réseau facultatifs : Python 3.10+ avec `paramiko` et `cryptography` déjà installés, puis `powershell -ExecutionPolicy Bypass -File tests/run-loopback.ps1 -Python python`. Le script crée ses propres fichiers sous `test-work`, utilise uniquement localhost et arrête la fixture en fin de test.

`--data-dir CHEMIN` sélectionne un carnet séparé pour les essais. L’exécutable compilé est `target\release\nexus-remote.exe`.

Références : [concepts Termius](https://docs.termius.com/getting-started/what-is-termius), [espaces](https://docs.termius.com/terminal/workspaces), [SFTP](https://docs.termius.com/organize-and-connect-to-hosts/managing-files-with-sftp), [egui](https://github.com/emilk/egui), [ssh2-rs](https://docs.rs/ssh2/), [RDP Microsoft](https://learn.microsoft.com/en-us/windows/win32/termserv/msrdpclient9), [niveaux d’authentification du serveur RDP](https://learn.microsoft.com/en-us/windows/win32/termserv/imsrdpclientadvancedsettings4-authenticationlevel), [configuration OpenSSH `GatewayPorts`](https://man.openbsd.org/sshd_config.5).

Les licences sont incluses dans LICENSE, THIRD_PARTY_NOTICES.md, THIRD_PARTY_LICENSES.txt et RUST_LIBRARY_COPYRIGHT.html. Les polices Windows sont lues depuis le système lorsqu’elles sont présentes et ne sont pas redistribuées.

## Licences

Le code applicatif est sous licence MIT. Les notices et textes de licences des dépendances sont fournis dans `THIRD_PARTY_NOTICES.md`, `THIRD_PARTY_LICENSES.txt` et `RUST_LIBRARY_COPYRIGHT.html`. Inter et JetBrains Mono sont sous SIL Open Font License 1.1 ; leurs textes originaux figurent aussi dans le dossier `licenses` du paquet Windows et `assets/fonts` des sources. Les polices de repli Windows sont lues sur le système et ne sont pas redistribuées.
