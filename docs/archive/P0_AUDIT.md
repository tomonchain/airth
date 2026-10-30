# Airth — P0 : audit du code face à SPECS_BUSINESS_AUTONOMY

**Date :** 28 octobre 2026  
**Base :** commit `b7d70f0` (Airth 0.1.5) et modifications locales non commitées  
**Référence :** [SPECS_BUSINESS_AUTONOMY.md](SPECS_BUSINESS_AUTONOMY.md)

## 1. Méthode et limites

Lecture ciblée des modules désignés au §15 de la spécification : realm, permissions, objective, scheduler, state, hub, ui (serveur, runs, approbations), usage, memory, notify, integration, tools/http, models/router.

Chaque exigence est classée :

- **Existe** : le mécanisme est présent et couvre l’exigence sur le fond.
- **Partiel** : un appui existe, mais une garantie demandée manque.
- **Absent** : rien d’équivalent.

Ce classement vient de la lecture du code, pas d’essais. Aucune garantie de reprise ou de concurrence n’a été vérifiée par un test d’incident. Les points marqués « à vérifier » doivent l’être par un test avant de s’appuyer dessus.

## 2. Constat transversal : le stockage

Tout l’état durable est en fichiers JSON par realm (`realm.json`, `objectives.json`, `projects.json`, `memory.json`, `runs/<id>.json`) et dans le home (`notifications.json`).

Chaque modification relit le fichier entier, le modifie en mémoire puis le réécrit via `write_atomic` (`src/realm.rs:583`) : écriture dans `<fichier>.tmp` puis `rename`.

Conséquences :

| Propriété | État | Détail |
| --- | --- | --- |
| Écriture d’un fichier non corrompue par un crash | Partiel | `rename` est atomique, mais sans `fsync` du fichier ni du dossier : après une coupure de courant, la dernière écriture peut être perdue. |
| Deux écritures concurrentes sur le même fichier | Partiel | `memory.rs` sérialise par un `Mutex` *dans le processus*. Ailleurs, rien. Le nom `.tmp` est fixe : deux écrivains simultanés peuvent écraser le temporaire l’un de l’autre, et la seconde écriture efface la première (perte de mise à jour). |
| Plusieurs processus | Absent | L’app (`airth serve`) et la CLI peuvent écrire les mêmes fichiers sans coordination. |
| Transaction couvrant plusieurs objets | Absent | Ex. : créer un run *et* mettre à jour `last_run` de l’objectif (`src/hub/objectives.rs:11`) sont deux écritures séparées. |
| Révision pour détecter une écriture concurrente (§14) | Absent | Aucun champ de version sur les objets. |

Les exigences §11.2 (réservations atomiques), §12.1 (baux), §12.2 (journal des actions) et §12.3 (déduplication cohérente avec la transition d’état) ne sont pas réalisables proprement sur ce socle.

**Recommandation :** SQLite (`rusqlite`, feature `bundled`) dans le home, pour les nouveaux objets métier uniquement. Mode WAL, transactions `BEGIN IMMEDIATE`, une colonne `revision` par objet mutable, une table d’audit append-only. Les stores JSON existants restent en place et lisibles ; aucune migration des données actuelles n’est nécessaire pour P1.

Pourquoi pas une alternative :

- *Garder JSON + verrous fichiers* : réinvente une base de données sans transactions multi-objets.
- *redb / sled* : pas de requêtes, moins inspectable et exportable (§13 exige un stockage inspectable).
- *Postgres* : contraire au local-first et ajoute un service à faire tourner.

Coût : une dépendance C compilée (SQLite embarqué), environ 1 Mo de binaire en plus.

## 3. Écarts par section de la spécification

### §4 Entreprise et §13 Séparation

| Exigence | État | Appui / manque |
| --- | --- | --- |
| Entreprise = realm enrichi | Partiel | Le realm existe (`src/realm.rs`) avec dossier, politique, secrets, mémoire, objectifs, projets. Pas de profil métier (offre, clientèle, standards). |
| Secrets séparés par entreprise | Existe | `realms/<name>/secrets.env`, lisible par le propriétaire seul ; injectés par le runtime, jamais vus par le modèle (`src/integration.rs`). |
| Politique séparée par entreprise | Existe | `Policy` dans `realm.json`, hors de portée des outils (`src/permissions.rs:1`). |
| Contexte modèle séparé | Partiel | Un run appartient à un realm et ne reçoit que son contexte. Le profil propriétaire est partagé seulement si le realm l’autorise (`src/owner.rs`). À vérifier : aucun outil (fs, shell, mac, mcp) ne permet de lire le dossier d’un autre realm. |
| Budgets séparés par entreprise | Absent | Les budgets sont par run ou par objectif (§11). |
| Vue portefeuille avec synthèses autorisées | Partiel | `scheduler::summary` agrège par realm ce qui s’est passé depuis une date. Rien ne filtre ce qu’un agent d’un realm verrait d’un autre, car aucun agent n’a de vue transverse aujourd’hui. |
| Mémoire : provenance et date | Existe | `MemoryEntry.source` (`owner` ou `run:<id>`) et `created_at` (`src/memory.rs:19`). |
| Mémoire : fait / hypothèse / décision distingués | Absent | Une entrée est un texte libre. |

### §5 Mandat versionné

| Exigence | État | Appui / manque |
| --- | --- | --- |
| Autorisations structurées appliquées par le runtime | Partiel | `PermissionGate::check` est le passage obligé de chaque appel d’outil (`src/runtime.rs`, `src/permissions.rs:254`). Mais la granularité est la *capacité* (read, write, execute, memory, network, skill, apps) avec une portée optionnelle (ex. `integration svc`), pas l’action métier ni ses paramètres (montant, destinataire). |
| Autorisations préalables pour le travail non surveillé | Partiel | `Objective.approvals: Vec<Grant>` (`src/objective.rs:61`). Liste plate, sans version, sans état `draft/active/superseded/revoked`. |
| Versions et cycle de vie du mandat | Absent | — |
| Référence du mandat sur chaque mission et action | Absent | Le run copie les grants de l’objectif au démarrage (`state.grants`, `src/hub/objectives.rs:32`). |
| Recontrôle avant chaque effet externe | Absent | Les grants copiés dans le run restent valides jusqu’à sa fin, même si l’objectif est modifié ou désactivé entre-temps. C’est l’écart le plus direct avec §5.2 et A-11. |
| Suspension sans arrêter les autres entreprises | Partiel | Un objectif peut être désactivé (`enabled`) ; un realm archivé n’est plus planifié. Les runs en cours ne sont pas affectés. |

### §6 Objectifs et mesures économiques

| Exigence | État | Appui / manque |
| --- | --- | --- |
| Objectif métier mesurable | Partiel | North Star = projet focus du realm (`src/projects.rs`, `src/northstar.rs`) : but, raison, jalons, tâches. Aucune mesure chiffrée, cible, échéance ni source. |
| Observations de mesure (période, source, fraîcheur, statut) | Absent | — |
| L’agent ne peut pas modifier sa mesure de succès | Partiel | Le but du North Star n’est modifiable que par le propriétaire. En revanche l’agent fait avancer les jalons via l’outil `north_star` ; un jalon « terminé » est déclaratif. |
| Confusion de nom avec `Objective` | À traiter | `Objective` dans le code = routine déclenchée. La spec doit fixer un terme d’interface pour l’objectif métier (proposition : « Cible »). |

### §7 Boucle de pilotage

| Exigence | État | Appui / manque |
| --- | --- | --- |
| Déclenchement sur cadence | Existe | `Trigger::Schedule` (`every 30m`, `daily 08:30`, `weekdays 09:00`…) ; un créneau manqué pendant l’arrêt ne se déclenche qu’une fois (`src/scheduler.rs:28`). |
| Déclenchement sur événement | Partiel | `Trigger::Watch` (empreinte de fichiers) et `Trigger::Webhook` (POST local sur `127.0.0.1`). Voir §12.3. |
| Choix et révision des missions par Airth | Absent | Un objectif lance un run avec un énoncé fixe. Aucune étape d’observation → comparaison → sélection de missions. |
| Pas de boucle LLM permanente | Existe | Le scheduler est déterministe (tick de 10 s, `src/ui/server.rs:206`) ; le modèle n’est appelé qu’au démarrage d’un run. |

### §8 Dossiers, missions, délégation

| Exigence | État | Appui / manque |
| --- | --- | --- |
| Dossier métier persistant | Absent | Les tâches de projet (`Task`) sont l’appui le plus proche : statut, lien vers les runs. Pas d’échéance, d’engagement ni de preuve. |
| Mission avec critères d’acceptation et états | Partiel | `Task` a `todo / in_progress / review / done` ; le run a `running / completed / failed / stopped` ; le plan de l’agent a des étapes `pending / in_progress / done / blocked`. Ni `waiting_owner`, ni `verifying`, ni critères d’acceptation exigés pour `succeeded`. |
| Délégation à des agents spécialisés | Absent | Un run = un agent. Aucun mécanisme de run enfant, de contexte restreint transmis ou de budget hérité. |
| Reprise par un autre modèle | Partiel | Un run peut changer de modèle (`model_choice`, `answered_by`) ; la compaction produit un résumé structuré (`src/agent.rs:425`). |

### §9 Contrôle de qualité

| Exigence | État | Appui / manque |
| --- | --- | --- |
| Contrôles programmatiques avant adoption | Existe (code) | Les checks du realm s’exécutent sur le worktree du run ; le propriétaire adopte ou rejette le diff (`src/isolation.rs`). Limité aux changements de code. |
| Grille versionnée par type de livrable | Absent | — |
| Le producteur ne peut pas abaisser la grille | Partiel | Les fichiers d’application des permissions sont signalés à l’adoption (`is_permission_critical`). Rien d’équivalent pour une grille métier puisqu’elle n’existe pas. |
| Corrections du propriétaire → cas de régression | Partiel | Airth Bench existe (`src/bench/`) avec realms isolés et vérifications indépendantes ; aucun chemin automatique d’une correction vers un cas. |

### §10 Décisions et relation avec le propriétaire

| Exigence | État | Appui / manque |
| --- | --- | --- |
| Correspondant unique | Existe | Une app, une inbox (`src/notify.rs`), des conversations par realm. |
| Proposition stratégique structurée | Absent | Les seules demandes au propriétaire sont des demandes de permission sur un appel d’outil. |
| Attente durable d’une décision | Absent | L’attente d’approbation d’un run non surveillé est en mémoire du processus (`recv_timeout`, `src/ui/live.rs:97`), bornée par `approval_wait_minutes` (10 par défaut), puis refus. Un redémarrage perd la demande. |
| « Le silence ne vaut pas accord » | Existe | Expiration = refus, jamais accord. |
| Briefing regroupé | Partiel | `scheduler::summary` produit un résumé par realm depuis une date : runs, objectifs, statut. Pas de coûts, d’écarts aux objectifs, ni de décisions attendues. |
| Règles d’urgence | Absent | Notifications sans priorité ni canal externe (pas de mail ni push). |

### §11 Budgets et autorisations

| Exigence | État | Appui / manque |
| --- | --- | --- |
| Plafonds d’exécution par run | Existe | `Budget { max_iterations, max_tokens, max_minutes }`, vérifié avant chaque appel modèle (`src/agent.rs:293`). |
| Plafonds par entreprise et par période | Absent | — |
| Coûts modèles mesurés | Partiel | `usage.rs` agrège tokens et coût *estimé* par jour, realm, modèle, run. Montants en `f64` ; §11.2 demande des unités mineures ou décimales. Pas de coût facturé. |
| Dépenses commerciales | Absent | — |
| Réservations atomiques | Absent | Bloqué par §2. |
| Accès générique ne contourne pas les contrôles | Partiel | `http_request` : secrets injectés par le runtime, une intégration ne peut envoyer son jeton qu’à sa base URL, adresses privées refusées hors intégration, redirections non suivies (`src/tools/http.rs`). Mais une fois `network: integration X` autorisé, *toutes* les méthodes et tous les chemins de l’API sont permis. Shell, MCP et skills : contrôle par capacité seulement. |
| Données externes non autoritatives | Partiel | Le mandat (objectifs, politique) est hors de portée des outils. Aucune marque de provenance sur le contenu injecté dans le contexte (payload webhook inclus tel quel dans le brief, `src/hub/objectives.rs:30`). |

### §12 Continuité et effets externes

| Exigence | État | Appui / manque |
| --- | --- | --- |
| Activité pendant l’absence | Partiel | `airth service` maintient `airth serve` au login via launchd (`src/service.rs`), macOS seulement. Machine éteinte = arrêt. |
| Reprise automatique après redémarrage | Absent | Un run resté `running` sans processus est affiché « interrupted » (`src/hub/runs.rs:176`) ; la reprise est une action du propriétaire (`airth resume`, bouton dans l’app). Un run non surveillé interrompu n’est jamais repris seul. |
| Verrou exclusif | Partiel | `RunLock` : fichier `<id>.lock` contenant le pid, considéré périmé si le processus est mort (`src/hub/runs.rs:195`). Test-puis-écriture non atomique ; pas de bail avec expiration ni de jeton empêchant une tentative obsolète d’agir. |
| Essais bornés et alternative sur panne fournisseur | Partiel | Le routeur essaie la connexion principale puis le fallback (`src/models/router.rs:93`). Si tous échouent, le run passe `failed` (`src/agent.rs:306`). Pas d’attente planifiée ni de nouvel essai différé. |
| Journal des actions externes avec états | Absent | Les événements de run (`state.events`) enregistrent action, arguments, résultat, `ok/error/denied`. Pas d’identifiant stable, de clé d’idempotence ni d’état `unknown`. |
| Événements dédupliqués | Absent | Le webhook démarre un run par POST ; aucun identifiant externe conservé. Si l’objectif tourne déjà, l’événement est refusé (409) et perdu. |
| Rattrapage de connecteur | Absent | — |

### §15 Architecture

| Exigence | État | Appui / manque |
| --- | --- | --- |
| Modèles interchangeables | Existe | `models/` : Anthropic, OpenAI-compatible, routeur rapide / principal / fallback. |
| Auto-modification sous isolation et adoption | Existe | Worktree par run, checks, adoption explicite, signalement des fichiers critiques (`src/isolation.rs`, `src/permissions.rs`). |
| Isolation hors macOS | À vérifier | `process_alive` renvoie toujours `true` hors Unix ; service et sandbox sont macOS. |

## 4. Couverture actuelle des scénarios de recette (§18.2)

| ID | Couverture | Ce qui manque d’abord |
| --- | --- | --- |
| A-01 Demande dans l’offre | Faible | Dossier, critères d’acceptation, grille. |
| A-02 Offre nouvelle suggérée | Nulle | Objet Proposition. |
| A-03 Pas de réponse du propriétaire | Faible | Expiration = refus existe ; décision durable absente. |
| A-04 Redémarrage pendant une mission | Faible | État conservé ; reprise automatique absente. |
| A-05 Effet externe au résultat inconnu | Nulle | Journal des actions, réconciliation. |
| A-06 Événement en double ou désordonné | Nulle | Déduplication, file d’événements. |
| A-07 Modèle indisponible | Partielle | Fallback existe ; attente et nouvel essai différé absents. |
| A-08 Budget partagé | Nulle | Budgets par entreprise, réservations. |
| A-09 Sources contradictoires | Nulle | Grille Hedgen. |
| A-10 Instruction externe malveillante | Partielle | Politique hors de portée, secrets non exposés, adresses privées bloquées. À mesurer avec Airth Bench (catégorie sécurité existante). |
| A-11 Révocation pendant exécution | Nulle | Recontrôle de l’autorisation avant effet. |
| A-12 Données absentes ou périmées | Nulle | Observations de mesure. |
| A-13 Bilan après 72 h | Faible | Résumé existe ; coûts, écarts, décisions attendues absents. |
| A-14 Deux entreprises | Partielle | Séparation des realms existe ; budgets par entreprise absents. |

A-04, A-05, A-06, A-07, A-08 et A-11 peuvent devenir des scénarios Airth Bench reproductibles avant P3.

## 5. Ce qui est déjà solide

- Le passage obligé de chaque outil par `PermissionGate` et le fait que la politique soit hors de portée de l’agent.
- Les secrets jamais exposés au modèle et liés à une base URL.
- La séparation par realm des fichiers, secrets, politiques et mémoire.
- Le scheduler déterministe, sans LLM permanent.
- L’état de run complet et rechargeable (messages, plan, grants, budget, origine).
- L’isolation par worktree et l’adoption explicite pour tout changement de code ou de skill.
- Airth Bench comme banc d’essai isolé et vérifié.

Le socle de sécurité et de séparation est la bonne base. Les manques sont concentrés sur la durabilité, les objets métier et l’autorisation par action.

## 6. Backlog proposé

Ordre guidé par la dépendance technique, puis par les scénarios débloqués. Chaque ligne vise une PR courte.

### Socle (avant P1, sans attendre les réponses Hedgen)

1. **Store SQLite métier** : ouverture, migrations versionnées, WAL, transaction, colonne `revision`, table d’audit append-only, export JSON. Tests de concurrence entre deux connexions.
2. **Mandat versionné** : table `mandate` (`draft → active → superseded | revoked`), activation par le propriétaire seulement (CLI + API), référence `mandate_id@version` sur chaque run lancé par un objectif.
3. **Recontrôle avant effet** : `PermissionGate` consulte le mandat actif pour les capacités à effet externe (write hors worktree, network vers intégration, apps, execute) au lieu des seuls grants copiés. Débloque A-11.
4. **Journal d’actions externes** : `prepared → in_flight → confirmed | failed | unknown`, écrit autour de `http_request` vers une intégration. Débloque l’instrumentation de A-05.

### P1 (après les réponses au questionnaire)

5. Profil métier du realm (offre, clientèle, standards) et terme d’interface pour l’objectif métier.
6. Dossier et Mission (états du §8.2, critères d’acceptation, preuves exigées pour `succeeded`).
7. Grille de qualité Hedgen versionnée, contrôlée par un vérificateur distinct du producteur.
8. Actions typées pour les connecteurs Hedgen retenus (paramètres contrôlés, pas seulement l’intégration).

### P2–P3

9. Observations de mesure et source de vérité économique.
10. Décisions et propositions durables (remplacent l’attente en mémoire de `src/ui/live.rs`).
11. Reprise automatique des missions admissibles au démarrage du serveur, avec bail et jeton de génération.
12. File d’événements entrants dédupliqués (le webhook écrit un événement, le scheduler le consomme).
13. Budgets par entreprise et par période, réservations, montants en unités mineures.
14. Briefing enrichi (coûts, écarts, échecs, décisions attendues) et canal d’urgence.
15. Scénarios Bench A-04 à A-08 et A-11.

## 7. Décisions à valider par le propriétaire

| Décision | Recommandation |
| --- | --- |
| Stockage des objets métier | SQLite embarqué, stores JSON existants inchangés. |
| Terme d’interface pour l’objectif métier | « Cible » (le code garde `Objective` pour les routines). |
| Démarrer le socle (1–4) avant les réponses Hedgen | Oui : il ne dépend d’aucun paramètre métier. |
