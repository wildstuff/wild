# contract-management-de — the ADR-0156 worked-example domain package

The **second** domain (after the liquidity tribe), so the adaptation walk's
domain-blindness is proven, not asserted. A domain package is a bundle of
**declarations** — never code, never a named connector, never a credential
(ADR-0156 D1):

| File | What it is |
|---|---|
| `package.yaml` | the manifest (frozen V1 schema — `wild_tribe_ops::domain_package`) |
| `ontology/model.yaml` | the domain model with a full **German reading** (ADR-0155: display names, field labels, authored button text, a sensitive IBAN value object) |
| `apps/vertrags-cockpit.yaml` | the default end-user door (an ADR-0154 app spec) |

Its sources ship as **unbound templates** (`unbound: true`, ADR-0156 OQ6):
the shape travels, the origin deliberately does not — after install the
adaptation walk asks for your folder/connection, and binding is one edit.

Everything here is pinned executable by the `domain_package` tests: the
manifest validates, the model compiles with the **unmodified** `wild-ddd`
compiler (the D1 criterion), the app template's bindings resolve against the
model, and the walk derives exactly the two blocking needs.
