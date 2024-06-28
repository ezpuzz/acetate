import asyncio
import os
from flask import Flask, render_template, request, session, redirect, url_for
import flask
from flask_htmx import HTMX, make_response
import flask_htmx
from flask_sqlalchemy import SQLAlchemy

from authlib.integrations.flask_client import OAuth
from jinja2 import StrictUndefined
import httpx

from dotenv import load_dotenv
import werkzeug
import werkzeug.datastructures
from pyroaring import BitMap
import base64
from elasticapm.contrib.flask import ElasticAPM
import re
from sqlalchemy.dialects.postgresql import insert

load_dotenv()

db = SQLAlchemy()

app = Flask(__name__, static_url_path="/public")
app.jinja_env.undefined = StrictUndefined

if os.environ.get("ELASTIC_APM_ENABLED") != "false":
    app.config["ELASTIC_APM"] = {
        "SERVICE_NAME": "acetate-frontend",
        "SECRET_TOKEN": os.environ.get("APM_SECRET_TOKEN"),
        "SERVER_URL": os.environ.get("APM_SERVER_URL"),
        "ENVIRONMENT": os.environ.get("RENDER_EXTERNAL_HOSTNAME"),
    }

    apm = ElasticAPM(app)


app.config["SQLALCHEMY_DATABASE_URI"] = os.environ.get("DATABASE_URL")

app.secret_key = os.environ.get("FLASK_SECRET_KEY", "supersekrit")

AXUM_API = os.environ.get("AXUM_API", "https://acetate.onrender.com/")

db.init_app(app)

with app.app_context():
    db.reflect()


class User(db.Model):
    __table__ = db.metadata.tables["users"]


class Action(db.Model):
    __table__ = db.metadata.tables["actions"]


htmx = HTMX(app)

oauth = OAuth(app)


def get_token():
    user = db.session.scalar(
        db.select(User).where(User.discogs_user_id == session.get("user").get("id"))
    )
    return dict(
        oauth_token=user.discogs_oauth_token,
        oauth_token_secret=user.discogs_oauth_token_secret,
    )


oauth.register(
    name="discogs",
    client_id=os.environ.get("DISCOGS_CLIENT_ID", None),
    client_secret=os.environ.get("DISCOGS_CLIENT_SECRET", None),
    request_token_url="https://api.discogs.com/oauth/request_token",
    access_token_url="https://api.discogs.com/oauth/access_token",
    authorize_url="https://www.discogs.com/oauth/authorize",
    fetch_token=get_token,
)


@app.route("/healthz")
def healthz():
    return "ok"


@app.errorhandler(404)
def page_not_found(error):
    return flask.redirect(url_for("discover"))


@app.route("/releases")
def releases():
    return flask.redirect(url_for("discover", **request.args))


@app.route("/")
@app.route("/discover")
async def discover():
    extra_args = {}

    if "videos_only" not in request.args:
        extra_args["videos_only"] = "on"

    args = request.args.copy()

    args.update(extra_args)

    async with asyncio.TaskGroup() as tg:
        filters = tg.create_task(get_filters())
        releases = tg.create_task(get_releases(args))

    filters = filters.result()
    releases, page, pageSize, offset, hits = releases.result()

    if htmx and not htmx.boosted:
        return render_template(
            "discover/results.jinja",
            **{
                "pageSize": pageSize,
                "releases": releases,
                "hits": hits,
                "page": page,
                "from": offset,
                "filters": filters,
                "htmx": htmx,
                **request.args,
            },
        )
    return render_template(
        "discover.jinja",
        **{
            "pageSize": pageSize,
            "releases": releases,
            "hits": hits,
            "page": page,
            "from": offset,
            "filters": filters,
            "htmx": htmx,
            **request.args,
        },
    )


@app.route("/dig")
async def dig():
    async with asyncio.TaskGroup() as tg:
        filters = tg.create_task(get_filters())
        releases = tg.create_task(get_releases(request.args, omit_hidden=False))

    filters = filters.result()
    releases, page, pageSize, offset, hits = releases.result()

    if htmx and not htmx.boosted:
        return render_template(
            "dig/results.jinja",
            **{
                "pageSize": pageSize,
                "releases": releases,
                "hits": hits,
                "page": page,
                "from": offset,
                "filters": filters,
                **request.args,
            },
        )
    return render_template(
        "dig.jinja",
        **{
            "pageSize": pageSize,
            "releases": releases,
            "hits": hits,
            "page": page,
            "from": offset,
            "filters": filters,
            **request.args,
        },
    )


@app.post("/want")
def want():
    if not request.form.get("release_id") or "user" not in session:
        return flask_htmx.make_response(redirect="/login")

    username = session["user"]["username"]

    wants = oauth.discogs.put(
        f"https://api.discogs.com/users/{username}/wants/{request.form.get('release_id')}",
        timeout=5,
    )
    wants.raise_for_status()
    return render_template("discover/wanted.jinja")


async def get_filters():
    filters = httpx.get(f"{AXUM_API}filters", timeout=10)
    filters.raise_for_status()
    return filters.json().get("aggregations")


async def get_releases(params: werkzeug.datastructures.MultiDict, omit_hidden=True):
    pageSize = int(params.get("pageSize", 5))
    offset = int(params.get("offset", (int(params.get("page", 1)) - 1) * pageSize))
    page = 1 + offset // pageSize

    actions = []
    if "user" in session and omit_hidden:
        actions = db.session.scalars(
            db.select(Action.identifier).where(
                Action.action == "HIDE",
                Action.user_id
                == db.select(User.user_id)
                .scalar_subquery()
                .where(User.discogs_user_id == session.get("user").get("id")),
            )
        ).all()

    params = [
        p
        for p in [
            (
                "hide",
                base64.urlsafe_b64encode(BitMap.serialize(BitMap(actions))).decode(
                    "UTF-8"
                ),
            )
            if actions
            else None,
            params.get("label") and ("field", "nested:labels.name"),
            params.get("label") and ("value", params["label"]),
            params.get("song") and ("field", "nested:tracklist.title"),
            params.get("song") and ("value", params["song"]),
            params.get("artist") and ("field", "nested:artists.name"),
            params.get("artist") and ("value", params["artist"]),
            params.get("title") and ("field", "title"),
            params.get("title") and ("value", params.get("title")),
            ("field", "nested:labels.catno") if params.get("catno") else None,
            ("value", re.sub(r"(.*?)\s*-?\s*(\d+)", r"\1 \2", params.get("catno")))
            if params.get("catno")
            else None,
            ("field", "nested:identifiers.value") if params.get("identifier") else None,
            ("value", params.get("identifier")) if params.get("identifier") else None,
            ("from", offset),
            ("size", pageSize),
            (
                "videos_only",
                "true" if params.get("videos_only") == "on" else "false",
            ),
            *[
                x
                for y in [
                    [
                        ("field", "formats.name"),
                        ("value", v),
                    ]
                    for v in params.getlist("formats.name")
                ]
                for x in y
            ],
            *[
                x
                for y in [
                    [
                        ("field", "formats.descriptions"),
                        ("value", v),
                    ]
                    for v in params.getlist("formats.descriptions")
                ]
                for x in y
            ],
            *[
                x
                for y in [
                    [("field", "styles"), ("value", v)]
                    for v in params.getlist("styles")
                ]
                for x in y
            ],
        ]
        if p
    ]

    releases = httpx.get(
        f"{AXUM_API}releases",
        params=params,
        timeout=10,
    )
    releases.raise_for_status()
    releases = releases.json()

    hits = int(releases["hits"]["total"]["value"])

    releases = [{**r["_source"], "id": r["_id"]} for r in releases["hits"]["hits"]]

    return releases, page, pageSize, offset, hits


async def hide_release(release_id):
    action = Action(
        user_id=db.select(User.user_id)
        .scalar_subquery()
        .where(User.discogs_user_id == session.get("user").get("id")),
        action="HIDE",
        identifier=release_id,
    )

    db.session.add(action)
    db.session.commit()


@app.post("/hide")
async def hide():
    if not request.form.get("release_id") or "user" not in session:
        return flask_htmx.make_response(redirect="/login")

    params = request.form.copy()
    params["pageSize"] = "1"
    params["offset"] = str(
        int(request.form.get("from", 0)) + int(request.form.get("pageSize", 5)) - 1
    )

    async with asyncio.TaskGroup() as tg:
        tg.create_task(hide_release(request.form.get("release_id")))
        filters = tg.create_task(get_filters())
        releases = tg.create_task(get_releases(params))

    filters = filters.result()
    releases, page, pageSize, offset, hits = releases.result()

    return render_template(
        "discover/hidden.jinja",
        **{
            "pageSize": pageSize,
            "releases": releases,
            "hits": hits,
            "page": page,
            "from": offset,
            "filters": filters,
            **request.form,
        },
    )


@app.route("/thumb/<release_id>")
def thumb(release_id):
    if "user" not in session:
        raise Exception()

    return render_template(
        "image.jinja",
        release_id=release_id,
        src=(
            oauth.discogs.get(
                f"https://api.discogs.com/releases/{release_id}", timeout=5
            )
            .json()
            .get("thumb")
        ),
    )


@app.route("/prices/<release_id>")
def prices(release_id):
    if "user" not in session:
        raise Exception()

    prices = oauth.discogs.get(
        f"https://api.discogs.com/marketplace/price_suggestions/{release_id}",
        timeout=5,
    ).json()

    if prices == {}:
        # look up price of master release
        release = httpx.get(f"{AXUM_API}release?id={release_id}", timeout=5).json()[
            "_source"
        ]
        if (
            "master_id" in release
            and release["master_id"]["is_main_release"] == "false"
        ):
            prices = oauth.discogs.get(
                f"https://api.discogs.com/marketplace/price_suggestions/{release['master_id']['#text']}",
                timeout=5,
            ).json()

    if "message" in prices:
        return prices["message"]

    short_prices = {}
    for k in prices:
        short_prices[k[k.index("(") + 1 : -1]] = prices[k]

    return render_template(
        "prices.jinja",
        prices=short_prices,
    )


@app.route("/wants")
def wants():
    username = session["user"]["username"]
    wants = oauth.discogs.get(
        f"https://api.discogs.com/users/{username}/wants", timeout=5
    )
    wants.raise_for_status()
    wants = wants.json()
    return render_template(
        "wants.jinja", **{"pageSize": 25, "wants": wants, **request.args}
    )


@app.route("/login")
def login():
    redirect_uri = url_for("auth", _external=True)
    return oauth.discogs.authorize_redirect(redirect_uri)


@app.route("/logout")
def logout():
    session.clear()
    return redirect("/")


@app.route("/auth")
def auth():
    token = oauth.discogs.authorize_access_token()
    resp = oauth.discogs.get("https://api.discogs.com/oauth/identity", timeout=5)
    user = resp.json()

    stmt = insert(User).values(
        discogs_oauth_token=token["oauth_token"],
        discogs_oauth_token_secret=token["oauth_token_secret"],
        discogs_user_id=user["id"],
        username=user["username"],
    )
    stmt = stmt.on_conflict_do_update(
        constraint="users_discogs_user_id_key",
        set_=dict(
            discogs_oauth_token=token["oauth_token"],
            discogs_oauth_token_secret=token["oauth_token_secret"],
        ),
    )
    db.session.execute(stmt)
    db.session.commit()

    session["user"] = user

    return redirect("/")
