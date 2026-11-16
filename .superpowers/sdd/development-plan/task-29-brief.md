# T29 — 包版本、发布内容与可复验消费脚本

Read first. Authority `docs/rust-sdk/development-plan.md` T29, `verification.md` §§9–10.1, decisionregisterE05/E10, actualCargo/pyproject/wheelCI. Controller GO. Version/packaging implementation happensnow; actualregistryqueries/resolution/package/consumerexecution DEFERREDuntilALLcodecomplete underuserinstruction. No publish/tag/merge/push.

## Scope

ThreeRustlibraryCargo metadata, internalpath+version andbindingdependencies, packageReadme/license/include/exclude andconsumer/manifests, releasecheck/evidence scripts, wheelbuildvsruntime matrix andversionsdocumentation. ChooseSDK first0.x candidatesourcedfromSDKmanifest independentlyprotocol/runtime3.x andPythonpackage3.x; provisionalcratenameavailabilityuntilactualregistryquerylater. Do notclaimnamefreeorregistrationpublished.

- Packageprotocol→runtime→SDK topology; explicit compatibleinternalversionswithpath+version, nopath-onlyproductiondep. Preservebindings compatibility andSDKPyO3-freedepclosure. Everycrate packagecontainsallrequiredsrc/license/readme, noabsolute/siblingunpackagedlink. Sourcearchives/wheels trackcandidatefullSHA/lock/feature/toolchain/hash.
- Preparecontentsverification/sourceunpackconsumer nopathpatchescapes; iflowercratesunpublished useexplicitlocalregistryorpatchingONLYunpackedlowerpackages+SDK. Recordpatchselectionfromactualconsumerlocklater; notclaimpublicregistrysuccess. Separatepublishdryrunwhenlowerrealversionsavailable. Publicregistryconsumer removesALLpath/patch/replace andusesSDK0.x, notworkspace3.x. Formalpublishcommandsdocs onlyneverexecute.
- Wheelbuildjobs mayuseRust/maturin; install/runtimejobs genuinelynobuildtoolsT28. Candidatewheel names/platform/ABI/hash andsourceversion relationships captured; nowheel=>fail,noSDistfallback. Pythonordinaryuserpipwheel noRust; sourcebuildersrequirements separatemanual. Supportmatrixdoesnotclaimunrunplatforms/MSRVpass.
- Scriptqueriescratename/versions read-onlylatervalidation, logsdatedURL/registryresponse; collisionresolveshonestpackage/lib namingorprivate-registrydecisionwithoutbreakingRustuseeltdx unintentionally. No fabricatedplaceholderavailable entry. Lockregen onlyaftercodecomplete, record pendingresolutionnow. Do notlaunchdependencyresolver/script as sourceinspection.

Authorpackaging edgecases/evidenceparsing/sourcecontent assertionsnecessary toensureconsumerdoesnotfallbacktorepogitpaths. Deferred actualcargopackage/list/publish--dry-run/registrylookup/unpackconsumer andwheelruntimeafterT30completion. Toolsabsent/platformunavailable→clearunverifiedevidence, notfakegreen.

Workspace `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`, branch `codex/rust-sdk-implementation`. UntilALL T01–T30codecomplete NO tests/collection/build/check/clippy/doctest/network/compile/AST/format/diffchecks/equivalents. Sourceedits/normalreviewdiff/Gitexactcheckpoint/hooksdisabledallowed. No subagents/push/tag/publish/merge/controllerstatusedits; coordinateCargo/CIroot. Report `.superpowers/sdd/development-plan/task-29-report.md`: exactversions/dep/package/wheelmatrixandprovisionalevidence, sourcescripts/exactfilesSHAs, finalvalidationcommands/gaps/noexecution/selfreview; compactfinal.
