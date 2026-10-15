"""App Store Connect, depuis GitHub Actions (aucun ordinateur nécessaire).

    python asc.py prepare   # l'identifiant de l'app, l'équipe, l'app existe-t-elle ?
    python asc.py testers   # le groupe de testeurs internes (toute l'équipe)
    python asc.py products  # les quatre périodes en achats intégrés (prix, textes)

La clé d'API vient de l'environnement : ASC_KEY_ID, ASC_ISSUER_ID, ASC_KEY
(le contenu du fichier .p8). Rien n'est affiché de la clé.
"""

import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

import jwt

API = "https://api.appstoreconnect.apple.com/v1"
BUNDLE_ID = os.environ.get("BUNDLE_ID", "com.maxlestage.alley")
APP_NAME = "Alley"
GROUP = "Alley"


def token():
    now = int(time.time())
    return jwt.encode(
        {"iss": os.environ["ASC_ISSUER_ID"], "iat": now, "exp": now + 15 * 60, "aud": "appstoreconnect-v1"},
        os.environ["ASC_KEY"].replace("\\n", "\n"),
        algorithm="ES256",
        headers={"kid": os.environ["ASC_KEY_ID"], "typ": "JWT"},
    )


def call(method, path, body=None, query=None, soft=False):
    url = (API.replace("/v1", "") + path) if path.startswith("/v2") else API + path
    if query:
        url += "?" + urllib.parse.urlencode(query)
    req = urllib.request.Request(url, method=method)
    req.add_header("Authorization", "Bearer " + token())
    data = None
    if body is not None:
        data = json.dumps(body).encode()
        req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, data) as r:
            raw = r.read()
            return json.loads(raw) if raw else {}
    except urllib.error.HTTPError as e:
        detail = e.read().decode(errors="replace")
        message = f"App Store Connect a refusé {method} {path} ({e.code}) : {detail[:600]}"
        if soft:
            print(f"::warning::{message}")
            return None
        raise SystemExit(message)


def output(name, value):
    path = os.environ.get("GITHUB_OUTPUT")
    if path:
        with open(path, "a") as f:
            f.write(f"{name}={value}\n")
    print(f"{name} = {value}")


def bundle():
    """L'identifiant de l'app, enregistré s'il ne l'est pas encore."""
    found = call("GET", "/bundleIds", query={"filter[identifier]": BUNDLE_ID, "limit": 5})["data"]
    found = [b for b in found if b["attributes"]["identifier"] == BUNDLE_ID]
    if found:
        return found[0]
    print(f"Enregistrement de l'identifiant {BUNDLE_ID}…")
    return call(
        "POST",
        "/bundleIds",
        {"data": {"type": "bundleIds", "attributes": {"identifier": BUNDLE_ID, "name": APP_NAME, "platform": "IOS"}}},
    )["data"]


def app():
    found = call("GET", "/apps", query={"filter[bundleId]": BUNDLE_ID, "limit": 5})["data"]
    return found[0] if found else None


def prepare():
    b = bundle()
    output("team_id", b["attributes"]["seedId"])
    a = app()
    if not a:
        print(
            "\n::error title=Créez l'app dans App Store Connect::"
            f"L'identifiant {BUNDLE_ID} est prêt, mais l'app n'existe pas encore dans App Store Connect. "
            "Sur appstoreconnect.apple.com : Apps → + → Nouvelle app → iOS, nom « Alley », "
            f"langue Français, identifiant de bundle {BUNDLE_ID}, SKU « alley ». Puis relancez ce workflow."
        )
        sys.exit(1)
    output("app_id", a["id"])


def testers():
    """Un groupe de testeurs internes qui reçoit chaque version : toute l'équipe."""
    a = app()
    if not a:
        raise SystemExit("App introuvable dans App Store Connect.")
    groups = call("GET", f"/apps/{a['id']}/betaGroups", query={"limit": 50})["data"]
    group = next((g for g in groups if g["attributes"].get("isInternalGroup") and g["attributes"]["name"] == GROUP), None)
    if not group:
        print(f"Création du groupe de testeurs internes « {GROUP} »…")
        group = call(
            "POST",
            "/betaGroups",
            {
                "data": {
                    "type": "betaGroups",
                    "attributes": {"name": GROUP, "isInternalGroup": True, "hasAccessToAllBuilds": True},
                    "relationships": {"app": {"data": {"type": "apps", "id": a["id"]}}},
                }
            },
        )["data"]
    users = call("GET", "/users", query={"limit": 50})["data"]
    present = {
        t["attributes"].get("email")
        for t in call("GET", f"/betaGroups/{group['id']}/betaTesters", query={"limit": 200})["data"]
    }
    for u in users:
        email = u["attributes"]["username"]
        if email in present:
            continue
        print(f"Ajout de {email} aux testeurs internes…")
        call(
            "POST",
            "/betaTesters",
            {
                "data": {
                    "type": "betaTesters",
                    "attributes": {
                        "email": email,
                        "firstName": u["attributes"].get("firstName") or "Alley",
                        "lastName": u["attributes"].get("lastName") or "Testeur",
                    },
                    "relationships": {"betaGroups": {"data": [{"type": "betaGroups", "id": group["id"]}]}},
                }
            },
        )
    print("Testeurs internes prêts : la version arrivera dans l'app TestFlight une fois traitée par Apple.")


# Les périodes : identifiant, prix en euros (France), nom et description
# (30 et 55 caractères au plus) en français, anglais, espagnol.
PERIODS = [
    ("3d", "4.99", {"fr-FR": ("3 jours", "Vos salons restent ouverts 3 jours."),
                    "en-US": ("3 days", "Your rooms stay open for 3 days."),
                    "es-ES": ("3 días", "Tus salas siguen abiertas 3 días.")}),
    ("7d", "9.99", {"fr-FR": ("7 jours", "Vos salons restent ouverts 7 jours."),
                    "en-US": ("7 days", "Your rooms stay open for 7 days."),
                    "es-ES": ("7 días", "Tus salas siguen abiertas 7 días.")}),
    ("1m", "14.99", {"fr-FR": ("1 mois", "Vos salons restent ouverts 1 mois."),
                     "en-US": ("1 month", "Your rooms stay open for 1 month."),
                     "es-ES": ("1 mes", "Tus salas siguen abiertas 1 mes.")}),
    ("3m", "34.99", {"fr-FR": ("3 mois", "Vos salons restent ouverts 3 mois."),
                     "en-US": ("3 months", "Your rooms stay open for 3 months."),
                     "es-ES": ("3 meses", "Tus salas siguen abiertas 3 meses.")}),
]
PREFIX = "com.maxlestage.alley.period."
REVIEW_NOTE = (
    "Paiement unique (pas d'abonnement) : pendant la période, les salons créés par ce compte "
    "restent ouverts au lieu de disparaître à minuit. Achat lié au compte Alley connecté."
)


def all_pages(path, query=None):
    out, url_query = [], dict(query or {}, limit=200)
    data = call("GET", path, query=url_query)
    while True:
        out += data.get("data", [])
        nxt = data.get("links", {}).get("next")
        if not nxt:
            return out
        rel = nxt.split("appstoreconnect.apple.com", 1)[1]
        rel = rel[3:] if rel.startswith("/v1") else rel
        data = call("GET", rel)


def screenshot(iap_id):
    """La capture d'écran pour la revue d'Apple (ci/review.png)."""
    import hashlib

    path = os.path.join(os.path.dirname(__file__), "review.png")
    raw = open(path, "rb").read()
    made = call("POST", "/inAppPurchaseAppStoreReviewScreenshots", {
        "data": {
            "type": "inAppPurchaseAppStoreReviewScreenshots",
            "attributes": {"fileName": "review.png", "fileSize": len(raw)},
            "relationships": {"inAppPurchaseV2": {"data": {"type": "inAppPurchases", "id": iap_id}}},
        }
    }, soft=True)
    if not made:
        return
    shot = made["data"]
    for op in shot["attributes"].get("uploadOperations") or []:
        req = urllib.request.Request(op["url"], method=op["method"], data=raw[op["offset"]:op["offset"] + op["length"]])
        for h in op.get("requestHeaders") or []:
            req.add_header(h["name"], h["value"])
        urllib.request.urlopen(req).read()
    call("PATCH", f"/inAppPurchaseAppStoreReviewScreenshots/{shot['id']}", {
        "data": {
            "type": "inAppPurchaseAppStoreReviewScreenshots", "id": shot["id"],
            "attributes": {"uploaded": True, "sourceFileChecksum": hashlib.md5(raw).hexdigest()},
        }
    }, soft=True)


def products():
    """Crée (s'ils manquent) les quatre achats intégrés, leurs textes, leur
    prix et leur disponibilité. Sans risque à relancer."""
    a = app()
    if not a:
        raise SystemExit("App introuvable dans App Store Connect.")
    existing = {p["attributes"]["productId"]: p for p in all_pages(f"/apps/{a['id']}/inAppPurchasesV2")}
    territories = [t["id"] for t in all_pages("/territories")]
    for plan, price, texts in PERIODS:
        pid = PREFIX + plan
        iap = existing.get(pid)
        if not iap:
            print(f"Création de l'achat intégré {pid}…")
            iap = call("POST", "/v2/inAppPurchases", {
                "data": {
                    "type": "inAppPurchases",
                    "attributes": {
                        "name": texts["fr-FR"][0], "productId": pid,
                        "inAppPurchaseType": "NON_RENEWING_SUBSCRIPTION", "reviewNote": REVIEW_NOTE,
                    },
                    "relationships": {"app": {"data": {"type": "apps", "id": a["id"]}}},
                }
            }, soft=True)
            if not iap:
                continue
            iap = iap["data"]
        iid = iap["id"]
        have = {l["attributes"]["locale"] for l in all_pages(f"/v2/inAppPurchases/{iid}/inAppPurchaseLocalizations")}
        for locale, (name, description) in texts.items():
            if locale not in have:
                call("POST", "/inAppPurchaseLocalizations", {
                    "data": {
                        "type": "inAppPurchaseLocalizations",
                        "attributes": {"locale": locale, "name": name, "description": description},
                        "relationships": {"inAppPurchaseV2": {"data": {"type": "inAppPurchases", "id": iid}}},
                    }
                }, soft=True)
        point = next((p for p in all_pages(f"/v2/inAppPurchases/{iid}/pricePoints", {"filter[territory]": "FRA"})
                      if p["attributes"]["customerPrice"] == price), None)
        if point:
            call("POST", "/inAppPurchasePriceSchedules", {
                "data": {
                    "type": "inAppPurchasePriceSchedules",
                    "relationships": {
                        "inAppPurchase": {"data": {"type": "inAppPurchases", "id": iid}},
                        "baseTerritory": {"data": {"type": "territories", "id": "FRA"}},
                        "manualPrices": {"data": [{"type": "inAppPurchasePrices", "id": "${price}"}]},
                    },
                },
                "included": [{
                    "type": "inAppPurchasePrices", "id": "${price}",
                    "attributes": {"startDate": None},
                    "relationships": {"inAppPurchasePricePoint": {"data": {"type": "inAppPurchasePricePoints", "id": point["id"]}}},
                }],
            }, soft=True)
        else:
            print(f"::warning::Pas de prix {price} € trouvé pour {pid}.")
        call("POST", "/inAppPurchaseAvailabilities", {
            "data": {
                "type": "inAppPurchaseAvailabilities",
                "attributes": {"availableInNewTerritories": True},
                "relationships": {
                    "inAppPurchase": {"data": {"type": "inAppPurchases", "id": iid}},
                    "availableTerritories": {"data": [{"type": "territories", "id": t} for t in territories]},
                },
            }
        }, soft=True)
        shot = call("GET", f"/v2/inAppPurchases/{iid}/appStoreReviewScreenshot", soft=True)
        if not (shot and shot.get("data")):
            screenshot(iid)
        print(f"{pid} : prêt ({price} €).")


if __name__ == "__main__":
    for name in ("ASC_KEY_ID", "ASC_ISSUER_ID", "ASC_KEY"):
        if not os.environ.get(name):
            raise SystemExit(f"Secret GitHub manquant : {name}")
    {"prepare": prepare, "testers": testers, "products": products}[sys.argv[1]]()
