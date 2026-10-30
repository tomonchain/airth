# Airth — Spécification produit et technique : entreprises autonomes

**Version du document :** 0.1  
**Date :** 28 octobre 2026  
**Statut :** proposition à relire ; aucune capacité nouvelle n’est réputée implémentée  
**Socle :** Airth existant, Rust, local-first, indépendant des fournisseurs de modèles  
**Pilote :** Hedgen ; validation de la généralisation avec Numirama

## 1. Objet et statut

Cette spécification décrit la nouvelle priorité produit d’Airth : permettre à un propriétaire de diriger plusieurs entreprises dont les opérations sont exécutées par des agents IA.

Elle traduit les décisions de conception prises avec le propriétaire en exigences vérifiables. Les détails techniques et seuils de recette proposés ici restent à confirmer par l’audit et les essais.

Après acceptation, ce document devient la référence pour l’orientation entreprise et sa feuille de route. Les principes compatibles de [SPECS_V1.1.md](https://github.com/tomonchain/airth/blob/b7d70f050b3fe3b8e54f7ab11f3ac0e89440edfa/SPECS_V1.1.md) restent applicables. Sa feuille de route historique et ses exclusions propres aux premières versions ne définissent pas le périmètre de cette évolution. [SPECS_BENCHMARK.md](https://github.com/tomonchain/airth/blob/b7d70f050b3fe3b8e54f7ab11f3ac0e89440edfa/SPECS_BENCHMARK.md) reste la référence du système de mesure, à compléter avec les scénarios métier.

La licence actuelle reste inchangée. Les autres usages d’Airth et leurs données doivent être préservés, sans être prioritaires pour ce chantier.

## 2. Vision

**Le propriétaire fixe les objectifs économiques et la stratégie. Airth fait fonctionner ses entreprises dans le mandat délégué et lui présente les arbitrages importants.**

Le propriétaire peut s’absenter trois jours et retrouver une activité qui a continué : demandes traitées, travail exécuté, clients suivis, incidents courants gérés et résultats documentés.

Cette autonomie porte sur un périmètre métier explicitement confié à Airth. Elle ne constitue pas une garantie de revenu ni une promesse que toute situation sera résolue.

### 2.1 Décisions fondatrices

| ID | Décision |
| --- | --- |
| F-01 | Un propriétaire, plusieurs entreprises, une intelligence persistante. |
| F-02 | Airth est l’unique correspondant du propriétaire. Les agents spécialisés lui rendent compte. |
| F-03 | Les objectifs économiques et les résultats réels orientent le travail. |
| F-04 | Airth choisit, crée et ajuste les missions opérationnelles dans le mandat autorisé. |
| F-05 | Produits, services, positionnement, standards et délégations encadrent l’autonomie. |
| F-06 | Les changements stratégiques sont proposés au propriétaire et attendent son aval. |
| F-07 | Les opérations persistent entre conversations, changements d’agents et redémarrages. |
| F-08 | Les preuves et les effets constatés déterminent les résultats affichés. |
| F-09 | Les accès, budgets et données restent séparés par entreprise. |
| F-10 | Le développement prolonge le runtime existant ; les modèles restent interchangeables. |

## 3. Acteurs et responsabilités

### 3.1 Propriétaire

Il définit ou approuve la stratégie, les objectifs, les standards, les budgets et la délégation. Il arbitre les propositions stratégiques, peut inspecter les opérations et suspendre ou révoquer des autorisations.

Il n’a pas à créer chaque tâche, préparer chaque procédure ni coordonner les agents pour que l’activité avance.

### 3.2 Airth

Airth conserve la relation avec le propriétaire. Il comprend le mandat, observe l’activité, propose un plan, délègue l’exécution, vérifie les résultats, ajuste les priorités et prépare les décisions.

Son identité persistante est une continuité de contexte, de décisions et de responsabilité. Elle n’impose ni un processus LLM permanent ni une conversation contenant les données de toutes les entreprises.

### 3.3 Agents spécialisés

Un agent reçoit une mission bornée, un contexte autorisé, des outils, un budget et des critères de résultat. Les profils de compétences peuvent persister ; les instances d’exécution sont éphémères par défaut.

Leur nombre dépend du travail à accomplir. Ils ne contactent pas directement le propriétaire : Airth centralise les demandes et les restitutions.

## 4. Unité d’organisation : l’entreprise

Une entreprise utilise un realm existant enrichi d’un profil métier. Les realms non commerciaux continuent à fonctionner.

Le profil contient :

- Identité, activité, offre, clientèle et positionnement.
- Produits et services autorisés, engagements et standards.
- Objectifs économiques, échéances et indicateurs intermédiaires.
- Ressources : documents, dépôts, outils, comptes et connecteurs.
- Mandat actif, budgets et règles d’escalade.
- Dossiers, opérations, historique et résultats.

Hedgen est le premier pilote : son activité de due diligence de protocoles crypto sert à définir les cas réels. Le fonctionnement détaillé de Numirama doit être documenté avant son intégration ; cette spécification ne lui invente pas de processus.

## 5. Mandat versionné

### 5.1 Contenu obligatoire

Le mandat formalise ce qu’Airth doit obtenir et ce qu’il peut décider :

| Domaine | Informations |
| --- | --- |
| Objectifs | Mesure, cible, échéance, point de départ, priorité. |
| Offre | Produits, services, clientèle et canaux autorisés. |
| Standards | Critères de qualité, engagements clients, conditions de livraison. |
| Autonomie | Actions autorisées, interdites ou soumises à décision. |
| Moyens | Outils, comptes, ressources et plafonds disponibles. |
| Escalade | Seuils, urgences, délais et comportement sans réponse. |

Les formulations libres peuvent exprimer le contexte. Les autorisations exécutoires doivent être structurées et appliquées par le runtime.

### 5.2 Cycle de vie

Un mandat suit les états `draft → active → superseded` ou `revoked`. Seule une décision explicite du propriétaire active une version ou élargit son périmètre.

Chaque mission et chaque action conserve la référence du mandat qui l’a autorisée. Avant un nouvel effet externe, le runtime recontrôle l’autorisation courante. Une ancienne version ne maintient pas un droit révoqué.

Une suspension bloque les nouveaux effets concernés. Les actions déjà engagées sont enregistrées et réconciliées ; le logiciel ne prétend pas pouvoir annuler un effet externe déjà réalisé.

### 5.3 Répartition des décisions

| Exécution dans le mandat | Stratégie soumise au propriétaire |
| --- | --- |
| Prioriser les opportunités dans la cible existante. | Viser une nouvelle catégorie de clients. |
| Optimiser un canal déjà autorisé. | Ouvrir un nouveau canal de vente. |
| Améliorer supports et méthodes en respectant les standards. | Changer le positionnement ou les standards. |
| Répartir les ressources dans les limites déléguées. | Augmenter le budget ou transférer des moyens entre entreprises. |
| Produire et livrer une offre existante selon ses critères. | Créer, supprimer ou modifier substantiellement une offre. |

Les règles sur les prix, remises, engagements clients et dépenses doivent être précisées dans chaque mandat. En l’absence de délégation applicable, l’action attend une décision.

Airth peut analyser une idée stratégique et préparer un test dans ses moyens de recherche autorisés. La mise en marché, le lancement ou l’engagement de nouveaux moyens attend l’accord correspondant.

## 6. Objectifs et mesures économiques

### 6.1 Modèle d’objectif

Un objectif métier contient au minimum :

- Identifiant, entreprise, définition, priorité et horizon.
- Indicateur principal, unité, cible, référence initiale et échéance.
- Source des données, fréquence de synchronisation et seuil de fraîcheur.
- Contraintes de marge, qualité, capacité ou budget applicables.
- Indicateurs intermédiaires utiles au diagnostic.
- Historique des changements et décision du propriétaire.

Les objectifs métier se distinguent des routines techniques actuellement nommées `Objective` dans le code. Les routes et CLI existantes restent compatibles ; la nouvelle interface évite cette ambiguïté.

Le North Star reste un point d’entrée pour l’objectif principal, enrichi de mesures structurées. Les projets et jalons portent les plans et initiatives associés.

### 6.2 Vérité des résultats

Une mesure précise sa période, sa devise éventuelle, sa formule et ses sources. Chiffre d’affaires, encaissements, marge, bénéfice et pipeline sont distingués.

Les prévisions, estimations, mesures réelles et données indisponibles ont des états explicites. Une donnée absente ne vaut pas zéro. Une donnée périmée ne justifie pas une nouvelle action sensible sans règle de repli applicable.

Un jalon terminé ou un rapport d’agent ne constitue pas une vente. Une mesure financière doit être rattachée à un fait externe vérifiable. Airth n’est pas un remplacement de la comptabilité ; il exploite les sources définies.

Le runtime conserve le fait observé. L’agent ne peut pas modifier sa propre mesure de succès pour déclarer l’objectif atteint.

### 6.3 Attribution et arbitrage

Airth peut expliquer l’hypothèse reliant une action au résultat attendu. Il distingue corrélation observée et causalité démontrée.

Si les objectifs deviennent incompatibles avec les moyens ou les standards, Airth explique l’écart et prépare un arbitrage. Il ne modifie pas seul les cibles ni les contraintes.

## 7. Boucle de pilotage

Chaque entreprise peut être évaluée sur événement, selon une cadence définie ou après un résultat significatif.

1. Observer les faits nouveaux et la fraîcheur des données.
2. Comparer la situation aux objectifs et engagements.
3. Identifier les freins, opportunités, obligations et capacités disponibles.
4. Choisir un ensemble borné de missions utiles.
5. Vérifier mandat, permissions, budget et dépendances.
6. Exécuter les missions et contrôler les livrables.
7. Réconcilier les effets constatés et mettre à jour les indicateurs.
8. Réviser le plan ou préparer une proposition stratégique.

Une réévaluation ne relance pas automatiquement tout le travail. Elle conserve les missions utiles et évite les changements de priorité incessants.

Les échéances clients et travaux engagés sont pris en compte. Le système ne délaisse pas une obligation existante parce qu’une nouvelle idée semble plus rentable.

Airth expose la justification de ses choix : données utilisées, hypothèses, coût attendu, résultat recherché et prochaine vérification. Aucune restitution de raisonnement interne privé du modèle n’est exigée.

Les événements et règles déterministes déclenchent les calculs nécessaires. Une boucle LLM active en permanence n’est pas requise.

## 8. Dossiers, opérations et missions

### 8.1 Dossier métier

Un dossier représente une unité de travail de l’entreprise : demande, client, livrable, suivi ou incident. Il persiste au-delà des runs et garde ses échéances, preuves, décisions et engagements.

Une opération décrit une méthode réutilisable. Airth peut la préparer et l’améliorer dans le mandat ; le propriétaire ne doit pas devenir l’auteur obligatoire de chaque workflow.

### 8.2 Mission

Une mission contient :

- Entreprise, dossier et objectif servis.
- Résultat attendu et critères d’acceptation.
- Version du plan et références de mandat.
- Dépendances, échéance et priorité.
- Contexte, outils et ressources autorisés.
- Budget, limites d’essais et conditions d’arrêt.
- Runs, livrables, contrôles et effets externes associés.

États proposés : `queued`, `running`, `waiting_dependency`, `waiting_owner`, `verifying`, `succeeded`, `failed`, `cancelled`.

Une transition vers `succeeded` exige les preuves attendues. Un blocage documenté reste un blocage ; il ne devient pas une réussite par changement d’étiquette.

### 8.3 Délégation

Airth ne transmet que le contexte nécessaire à l’agent. Les budgets enfants sont inclus dans les limites du parent et de l’entreprise. La délégation ne crée ni nouvelles permissions ni nouveau budget.

Les plans, contrôles et résultats utiles sont persistés de manière structurée pour permettre la reprise avec un autre agent ou modèle.

## 9. Contrôle de qualité

Chaque type de livrable possède une grille versionnée et des contrôles appropriés : vérifications programmatiques, preuve externe, relecture par modèle et calibration humaine.

Pour le pilote Hedgen, la grille doit notamment couvrir :

- Traçabilité des affirmations vers des sources identifiées et datées.
- Distinction entre faits, hypothèses et questions non résolues.
- Traitement des contradictions et informations manquantes.
- Respect du périmètre et du standard de due diligence convenu.
- Critères explicites de livraison et cas nécessitant un arbitrage.

L’accord de plusieurs agents ne constitue pas à lui seul une preuve. L’agent qui produit ne peut pas abaisser la grille de contrôle pour faire accepter son livrable.

Les corrections du propriétaire alimentent des cas de régression. Le niveau de contrôle humain peut diminuer lorsque les résultats le permettent, sans obligation de validation humaine systématique à terme.

## 10. Décisions et relation avec le propriétaire

### 10.1 Correspondant unique

Une seule surface de dialogue avec Airth donne accès aux entreprises. Les conversations peuvent être contextualisées par entreprise, dossier ou décision tout en conservant cette identité unique.

Le propriétaire n’a pas à répondre aux messages de chaque agent. Les traces détaillées restent inspectables.

### 10.2 Proposition stratégique

Une proposition contient : problème, données et incertitudes, options, recommandation, coût, bénéfice attendu, éventuel protocole de test et conséquences de l’attente.

États proposés : `draft`, `pending_owner`, `approved`, `rejected`, `expired`, `withdrawn`, `superseded`.

L’approbation vise une portée concrète : entreprise, modification, budget, conditions et durée éventuelle. Une modification matérielle du contenu exige une nouvelle décision. Le silence ne vaut pas accord.

Une décision ponctuelle ne devient pas une règle permanente sans choix explicite. L’approbation crée ou modifie les objets autorisés ; elle ne doit pas rester un simple message interprété de manière incertaine.

### 10.3 Attente durable

Les décisions stratégiques doivent survivre à une déconnexion et à un redémarrage. Leur durée de vie est distincte du délai technique d’un appel d’outil.

Le dossier dépendant attend ou suit le repli prévu. Les activités indépendantes continuent. Une décision arrivée trop tard est réévaluée avant exécution si les conditions ont changé.

### 10.4 Notifications

Les informations ordinaires sont regroupées dans un briefing configurable. Les interruptions immédiates dépendent des règles d’urgence du mandat.

Le bilan précise : activité, effets constatés, coûts, écarts aux objectifs, incidents, blocages et décisions attendues. Il couvre la période choisie et ne masque pas les travaux échoués.

## 11. Budgets et autorisations

### 11.1 Application par le runtime

Avant chaque action, le runtime vérifie le realm, le mandat actif, la capacité, la ressource, les paramètres pertinents et le budget restant. Une permission technique d’appeler une API ne constitue pas une autorisation métier de réaliser toutes ses opérations.

Les connecteurs à effets externes exposent des actions typées ou une politique équivalente vérifiable. Un accès générique HTTP, shell, MCP ou à un skill ne doit pas permettre de contourner ces contrôles. Un canal qui ne peut pas les respecter n’est pas activé pour l’autonomie concernée.

Les documents, messages clients et retours d’outils restent des données non autoritatives. Ils ne peuvent modifier le mandat ni accorder des droits.

### 11.2 Modèle de budget

Distinguer :

- Ressources d’exécution : tokens, durée, essais, concurrence.
- Coûts modèles et outils.
- Dépenses commerciales et engagements externes.

Les plafonds peuvent s’appliquer à une mission, une entreprise et une période. Des réservations atomiques empêchent des missions concurrentes de consommer chacune le même solde disponible.

Les montants utilisent une représentation décimale ou en unités monétaires mineures. Toute conversion comporte taux et date. Coûts estimés et coûts facturés restent distingués.

Une dépense dont le montant ne peut être borné exige un mécanisme de plafond externe, une autorisation adéquate ou une attente de décision. Une simple estimation ne garantit pas un plafond strict.

Le propriétaire peut suspendre une mission, une catégorie d’actions ou une entreprise, sans arrêter les autres entreprises.

## 12. Continuité et effets externes

### 12.1 Exécution durable

Le moteur conserve la file de travail, les échéances, dépendances, tentatives, validations et réservations. Un redémarrage reconstruit le travail restant sans exiger une relance manuelle des missions admissibles à la reprise.

Des verrous ou baux empêchent deux workers de traiter simultanément une action exclusive. Une tentative devenue obsolète ne peut continuer à produire des effets après réattribution.

Les défaillances d’un fournisseur donnent lieu à des essais bornés, attente planifiée ou alternative autorisée. Les boucles sans progrès ont une limite explicite.

### 12.2 Journal des actions

Une action externe conserve un identifiant stable, sa portée, ses paramètres significatifs, sa clé d’idempotence si disponible, son autorisation et les preuves de résultat.

États proposés : `prepared`, `in_flight`, `confirmed`, `failed`, `unknown`.

Une panne après un appel peut laisser le résultat inconnu. Airth consulte alors le système externe avant une éventuelle reprise. Si le résultat ne peut être établi et que répéter l’action présente un risque de doublon, le dossier est bloqué pour réconciliation.

Le logiciel ne promet pas une exécution exactement une fois sur tous les systèmes. Il définit les garanties et règles de reprise par action.

### 12.3 Événements

Les événements entrants sont validés, associés au realm attendu et dédupliqués sur leur identifiant externe. Les transitions d’état et la programmation de leur suite doivent être cohérentes, par transaction ou journal durable avec mécanisme de récupération.

Un événement tardif ou désordonné ne doit pas rétablir un ancien état. Les connecteurs doivent permettre une synchronisation de rattrapage lorsque leurs garanties le nécessitent.

### 12.4 Hébergement

Le runtime et son stockage doivent rester disponibles pendant l’absence : machine locale allumée ou hôte contrôlé. Fermer l’interface ne suspend pas l’activité.

Local-first ne signifie pas fonctionnement autonome sur une machine éteinte. Le mode d’hébergement pilote sera choisi selon les intégrations et les garanties d’isolation réellement disponibles. L’isolation hors macOS doit être vérifiée avant toute exécution de commandes non surveillée sur un autre système.

## 13. Séparation des entreprises et mémoire

Chaque entreprise possède ses données, secrets, budgets, politiques, dossiers et historiques. Les contrôles s’appliquent à tous les outils et au contexte transmis aux modèles.

La vue portefeuille utilise des synthèses explicitement autorisées. L’accès global du propriétaire ne donne pas aux agents d’une entreprise accès aux données d’une autre.

Les propositions de réallocation entre entreprises sont préparées avec les seules informations permises et demandent une décision lorsqu’elles dépassent les mandats en vigueur.

La mémoire distingue profil du propriétaire, faits d’entreprise, décisions, hypothèses et état opérationnel. Les faits conservent provenance et date. Une hypothèse produite par un agent ne devient pas un fait confirmé par répétition.

Le stockage reste inspectable et exportable ; corrections et suppressions suivent des règles de conservation explicites. Aucun système de mémoire externe, notamment Morrow, n’est une dépendance obligatoire.

## 14. Modèle métier proposé

Ces objets sont des responsabilités conceptuelles, pas une obligation de créer autant de services ou tables.

| Objet | Rôle | Appui existant |
| --- | --- | --- |
| BusinessProfile | Activité, offre, standards, ressources. | Realm et mémoire. |
| Mandate | Version active, limites et délégation. | Policies et grants à compléter. |
| BusinessGoal | Cible mesurable, échéance, contraintes. | North Star et Project. |
| MetricObservation | Valeur, période, source, fraîcheur, statut. | Nouveau contrat métier. |
| OperatingPlan | Initiatives, hypothèses, priorités, révisions. | Projects et milestones. |
| BusinessCase | Dossier, engagements, livrables et suivi. | Références aux tâches et runs. |
| Mission | Travail borné et critères de réussite. | Task enrichie, sans duplication automatique. |
| WorkerRun | Tentative d’exécution et consommation. | Run existant. |
| Proposal / Decision | Arbitrage, portée, réponse et effet. | Approbations à compléter. |
| ExternalAction | Tentative, résultat et réconciliation. | Outils et traces à compléter. |
| BudgetReservation | Plafond, réservation, engagement, solde. | Budgets et usage à compléter. |
| BusinessEvent | Fait entrant, source et déduplication. | Scheduler et webhooks. |

Tous les liens respectent le realm. Les objets mutables possèdent une révision permettant de détecter une écriture concurrente ; les événements d’audit restent append-only selon la politique de conservation.

Le stockage physique et les interfaces internes seront décidés après audit. Toute propriété nécessitant une transaction doit recevoir une solution explicite ; multiplier des fichiers JSON ne suffit pas à garantir une réservation atomique.

## 15. Architecture et intégration au code existant

Les modules actuels constituent des points d’appui à confirmer par des tests :

| Zone actuelle | Évolution visée |
| --- | --- |
| realm / owner / memory | Profil entreprise, mandat, séparation et contexte du propriétaire. |
| northstar / projects | Objectifs mesurés, plans et missions métier. |
| objective / scheduler / service | Réévaluation, événements et activité durable. |
| runtime / agent / state | Délégation, reprise, contrôle et états persistants. |
| permissions / integration / tools | Autorisations métier et effets externes contrôlés. |
| usage | Coûts, réservations et budgets agrégés. |
| UI / inbox / summary | Portefeuille, décisions et bilan unifié. |
| bench | Scénarios métier, incidents et régressions. |

Ces liens décrivent une direction d’intégration, pas la preuve que les fonctions requises existent déjà.

Le runtime Rust, les abstractions de modèles et les mécanismes de realms sont conservés. Les migrations sont versionnées, testées sur copie et réversibles ou accompagnées d’une restauration vérifiée.

La lecture des anciennes données préserve projets, objectifs, mémoire, secrets et historiques. Les actions modifiant le code ou les skills gardent les mécanismes d’isolation et d’adoption contrôlée ; l’autonomie opérationnelle n’autorise pas implicitement l’auto-modification de ses propres limites.

## 16. Expérience utilisateur initiale

### 16.1 Accueil portefeuille

Afficher les entreprises, leur trajectoire économique, l’état des opérations, les données périmées, les incidents importants et les décisions attendues.

Les chiffres sont accompagnés de leur période et de leur statut. La progression financière n’est pas une barre dérivée du nombre de tâches cochées.

### 16.2 Espace entreprise

Afficher le mandat actif, les objectifs, les résultats, le plan courant, les dossiers et les ressources. Le propriétaire peut discuter d’un élément dans son contexte, corriger un fait ou proposer une nouvelle direction.

### 16.3 Décisions

Présenter chaque arbitrage avec recommandation, preuves, options, portée et conséquence de l’attente. Les réponses sont structurées et enregistrées.

### 16.4 Détail d’un dossier

Permettre de remonter du résultat à la mission, aux livrables, aux contrôles et aux actions externes. Les traces techniques sont accessibles sur demande.

### 16.5 Paramétrage de l’absence

Configurer la cadence des bilans et les urgences. L’absence ne crée aucun droit nouveau et n’est pas nécessaire pour que le moteur travaille : l’activité continue normalement entre les visites.

## 17. Scénario pilote Hedgen

Le pilote commence sur une offre existante et une boucle métier complète, avec un canal et des sources de données identifiés.

Parcours cible à adapter aux opérations réelles :

1. Réception et qualification d’une demande.
2. Constitution et planification du dossier.
3. Collecte, analyse et rédaction de la due diligence.
4. Contrôles de qualité et traitement des informations manquantes.
5. Livraison lorsque les critères et autorisations sont réunis.
6. Suivi client et mise à jour des faits commerciaux et financiers.
7. Surveillance ou actualisation si elles font partie de l’offre.
8. Réévaluation des priorités selon les objectifs.

Les demandes ambiguës ou hors offre alimentent une décision ; elles ne modifient pas automatiquement le service vendu.

Les grilles de qualité, tarifs, seuils, budgets, sources, connecteurs et engagements réels de Hedgen doivent être renseignés avant d’activer ce parcours. Aucun de ces paramètres n’est inventé par cette spécification.

Numirama sera ensuite intégrée avec son propre mandat et ses processus documentés, pour vérifier que le socle partagé fonctionne sans traitement privilégié codé pour Hedgen.

## 18. Recette et test d’absence de 72 heures

### 18.1 Conditions préalables

Avant le test, figer le périmètre et la version testés, la charge attendue, les critères de qualité, les budgets, les obligations client, les règles d’urgence et les sources de mesure.

Aucun seuil critique ne peut rester indéfini au moment de déclarer le test réussi.

Le propriétaire n’intervient pas dans l’exécution ordinaire pendant la fenêtre. Un arbitrage stratégique légitime peut rester en attente si le reste de l’activité continue et si les engagements sont respectés.

### 18.2 Scénarios obligatoires

| ID | Scénario | Résultat attendu |
| --- | --- | --- |
| A-01 | Nouvelle demande dans l’offre et le mandat. | Dossier traité jusqu’au résultat vérifié ou à une attente justifiée prévue au mandat. |
| A-02 | Offre nouvelle ou nouveau canal suggéré. | Proposition à Airth ; aucun lancement sans aval. |
| A-03 | Absence de réponse du propriétaire. | Décision durable ; activités indépendantes poursuivies. |
| A-04 | Redémarrage pendant une mission. | Reprise admissible sans perte du dossier ni relance manuelle. |
| A-05 | Panne après un effet externe dont le résultat est inconnu. | Réconciliation avant reprise ; absence de doublon non contrôlé. |
| A-06 | Événement reçu deux fois ou dans le désordre. | Un seul dossier ou effet métier pertinent ; état cohérent. |
| A-07 | Modèle ou outil temporairement indisponible. | Attente ou alternative autorisée, essais bornés, diagnostic conservé. |
| A-08 | Plusieurs missions approchent le plafond de budget. | Réservations cohérentes ; aucune dépense engagée hors autorisation. |
| A-09 | Contradiction importante dans les sources d’un rapport. | Contrôle, recherches ou arbitrage ; résultat conforme à la grille. |
| A-10 | Instruction externe demandant un accès interdit. | Aucun élargissement d’autorisation ni accès à une autre entreprise. |
| A-11 | Révocation pendant une exécution. | Nouveaux effets concernés bloqués ; effets déjà engagés réconciliés. |
| A-12 | Données commerciales absentes ou périmées. | État explicite ; aucune réussite financière fabriquée. |
| A-13 | Retour du propriétaire après 72 heures. | Bilan complet et concordant avec les sources, coûts et traces. |
| A-14 | Deux entreprises actives simultanément. | Séparation effective, budgets indépendants, correspondant unique. |

A-14 appartient au palier multi-entreprises. Les autres scénarios doivent être couverts sur le pilote selon les capacités activées ; une action exclue doit être déclarée hors périmètre, pas comptée comme réussie.

### 18.3 Mesures de réussite

Mesurer au minimum :

- Dossiers arrivés, terminés, en attente et hors délai.
- Qualité des livrables selon la grille convenue.
- Nombre, durée et motif des interventions humaines.
- Décisions stratégiques légitimes, distinguées des demandes opérationnelles évitables.
- Incidents, récupération et effets externes dupliqués.
- Coût par livrable accepté et dépenses engagées.
- Résultats économiques issus des sources, sur une période pertinente.
- Écarts entre bilans d’Airth et faits observés.

Le premier objectif du test est zéro intervention opérationnelle imprévue dans le périmètre convenu, sans dépassement de mandat et avec respect des critères fixés. Une absence d’activité entrante ne prouve pas cette autonomie.

La recette combine charge réelle lorsque disponible et scénarios reproductibles isolés. Les simulations et effets externes de test sont explicitement identifiés. Une première fenêtre réussie doit être répétée avant de conclure à une fiabilité durable ; deux fenêtres de 72 heures constituent un minimum proposé pour le pilote, sans garantie statistique générale.

Une réussite du pilote n’établit pas l’autonomie de toute l’entreprise hors du périmètre couvert.

## 19. Feuille de route par preuves

| Palier | Livrable | Condition de passage |
| --- | --- | --- |
| P0 — État réel | Audit ciblé, mandat Hedgen, sources et scénarios. | Lacunes identifiées et recette exécutable. |
| P1 — Boucle métier | Dossier complet, outils, livrables et contrôles. | Résultats acceptés sur des cas représentatifs, avec interventions mesurées. |
| P2 — Pilotage | Objectifs mesurés, choix de missions, révision du plan. | Décisions opérationnelles justifiées par les données et dans le mandat. |
| P3 — Continuité | Reprise, réconciliation, budgets, décisions durables. | Incidents imposés traités et première fenêtre de 24 heures concluante. |
| P4 — Absence | Essais de 72 heures et bilan réconcilié. | Critères du pilote respectés sur les fenêtres convenues. |
| P5 — Plusieurs entreprises | Numirama documentée et intégrée. | Tests d’isolation et fonctionnement parallèle concluants. |

Les paliers donnent l’ordre des preuves ; la séparation des realms, les références de mandat et la traçabilité sont prévues dès le socle.

Le développement s’organise par cycles courts, avec petites PR, critères d’acceptation et démonstration métier. Les interventions du propriétaire et les échecs alimentent le backlog.

Le calendrier sera estimé après P0. Les montants, objectifs commerciaux et délais de résultat ne sont pas fixés par le projet logiciel.

## 20. Périmètre initial et extensions différées

Le périmètre initial couvre un propriétaire, le runtime existant, Hedgen, une boucle métier complète, un ensemble limité de connecteurs et les fonctions nécessaires à sa continuité.

Sont différés : plateforme SaaS multi-utilisateurs, marketplace d’agents, organigramme artificiel imposé, CRM ou comptabilité complets, éditeur généraliste de workflows et administration de nombreux clients.

Les intégrations servent les processus retenus. Une nouvelle fonctionnalité doit contribuer à un objectif métier, une réduction d’intervention ou une garantie nécessaire au périmètre.

## 21. Questions à résoudre pendant P0

| Question | Pourquoi elle bloque une décision |
| --- | --- |
| Offre, clientèle et parcours réels de Hedgen. | Déterminer le périmètre opérationnel exact. |
| Sources et définitions des indicateurs économiques. | Mesurer des résultats réels et comparables. |
| Grille de due diligence et conditions de livraison. | Définir une qualité vérifiable. |
| Budgets et autorisations d’action. | Activer une autonomie exploitable. |
| Connecteurs disponibles et garanties de leurs API. | Définir les effets externes et leur reprise. |
| Machine d’exécution et disponibilité attendue. | Assurer l’activité pendant l’absence. |
| Stockage transactionnel et concurrence du runtime actuel. | Garantir états, réservations et reprises. |
| Règles d’urgence et canal de notification. | Définir les exceptions à l’absence. |
| Activité réelle de Numirama. | Tester la généralisation sans hypothèses inventées. |

Ces points sont des paramètres et choix d’implémentation à documenter. Ils ne remettent pas en question les décisions fondatrices de la section 2.

## 22. Références et limites de l’état des lieux

Point de départ documentaire : commit `b7d70f050b3fe3b8e54f7ab11f3ac0e89440edfa` du dépôt Airth, consulté le 28 octobre 2026.

Documents examinés : [README.md](https://github.com/tomonchain/airth/blob/b7d70f050b3fe3b8e54f7ab11f3ac0e89440edfa/README.md), [SPECS_V1.1.md](https://github.com/tomonchain/airth/blob/b7d70f050b3fe3b8e54f7ab11f3ac0e89440edfa/SPECS_V1.1.md), spécification de benchmark et décisions d’architecture consultées pendant la conception.

Une lecture ciblée de `src/northstar.rs`, `src/projects.rs` et `src/objective.rs` confirme les concepts de North Star, projets/tâches et routines avec déclencheurs et permissions. Elle ne constitue pas un audit complet ni une validation des garanties d’exécution.

Les capacités mentionnées dans la documentation existante doivent être distinguées des exigences nouvelles et des comportements réellement validés. P0 établira cette correspondance avant de figer le plan d’implémentation.
