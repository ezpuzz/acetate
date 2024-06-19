import os
from flask import Flask, render_template, request, session, redirect, url_for
from flask_htmx import HTMX
from flask_sqlalchemy import SQLAlchemy

from authlib.integrations.flask_client import OAuth
from jinja2 import StrictUndefined
import requests

from dotenv import load_dotenv

load_dotenv()

db = SQLAlchemy()

app = Flask(__name__, static_url_path="/public")
app.jinja_env.undefined = StrictUndefined

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


@app.route("/")
@app.route("/releases")
def releases():
    filters = requests.get(f"{AXUM_API}filters", timeout=5)
    filters.raise_for_status()

    pageSize = int(request.args.get("pageSize", 5))
    offset = (int(request.args.get("page", 1)) - 1) * pageSize
    page = 1 + offset // pageSize

    releases = requests.get(
        f"{AXUM_API}releases",
        params=[
            p
            for p in [
                ("field", "nested:labels.name")
                if "label" in request.args and request.args["label"]
                else None,
                ("value", request.args["label"])
                if "label" in request.args and request.args["label"]
                else None,
                ("field", "nested:tracklist.title")
                if "song" in request.args and request.args["song"]
                else None,
                ("value", request.args["song"])
                if "song" in request.args and request.args["song"]
                else None,
                ("field", "nested:artists.name")
                if "artist" in request.args and request.args["artist"]
                else None,
                ("value", request.args["artist"])
                if "artist" in request.args and request.args["artist"]
                else None,
                ("from", offset),
                ("size", pageSize),
                (
                    "videos_only",
                    "true"
                    if request.args.get("videos_only", "on") == "on"
                    else "false",
                ),
                *[
                    x
                    for y in [
                        [
                            ("field", "formats.name.keyword"),
                            ("value", v),
                        ]
                        for v in request.args.getlist("formats.name.keyword")
                    ]
                    for x in y
                ],
                *[
                    x
                    for y in [
                        [
                            ("field", "formats.descriptions.keyword"),
                            ("value", v),
                        ]
                        for v in request.args.getlist("formats.descriptions.keyword")
                    ]
                    for x in y
                ],
                *[
                    x
                    for y in [
                        [("field", "styles.keyword"), ("value", v)]
                        for v in request.args.getlist("styles.keyword")
                    ]
                    for x in y
                ],
            ]
            if p is not None
        ],
        timeout=10,
    )
    print(releases.url)
    releases.raise_for_status()
    releases = releases.json()

    hits = int(releases["hits"]["total"]["value"])

    releases = [r["_source"] for r in releases["hits"]["hits"]]

    for r in releases:
        if "videos" in r:
            r["videos"] = [v[32:] for v in r["videos"]]

    if htmx and not htmx.boosted:
        return render_template(
            "releases/results.jinja",
            **{
                "pageSize": pageSize,
                "releases": releases,
                "hits": hits,
                "page": page,
                "from": offset,
                "filters": filters.json(),
                **request.args,
            },
        )
    return render_template(
        "index.jinja",
        **{
            "pageSize": pageSize,
            "releases": releases,
            "hits": hits,
            "page": page,
            "from": offset,
            "filters": filters.json(),
            **request.args,
        },
    )


@app.post("/want")
def want():
    if not request.form.get("release_id"):
        raise Exception()

    username = session["user"]["username"]

    wants = oauth.discogs.put(
        f"https://api.discogs.com/users/{username}/wants/{request.form.get('release_id')}",
        timeout=5,
    )
    wants.raise_for_status()
    return render_template("releases/wanted.jinja")


@app.post("/hide")
def hide():
    if not request.form.get("release_id"):
        raise Exception()

    action = Action(
        user_id=db.select(User.user_id)
        .scalar_subquery()
        .where(User.discogs_user_id == session.get("user").get("id")),
        action="HIDE",
        identifier=request.form.get("release_id"),
    )

    db.session.add(action)
    db.session.commit()

    filters = requests.get(f"{AXUM_API}filters", timeout=5)
    filters.raise_for_status()

    pageSize = int(request.form.get("pageSize", 5))
    offset = (int(request.form.get("page", 1)) - 1) * pageSize
    page = 1 + offset // pageSize

    releases = requests.get(
        f"{AXUM_API}releases",
        params=[
            p
            for p in [
                ("field", "nested:labels.name")
                if "label" in request.form and request.form["label"]
                else None,
                ("value", request.form["label"])
                if "label" in request.form and request.form["label"]
                else None,
                ("field", "nested:tracklist.title")
                if "song" in request.form and request.form["song"]
                else None,
                ("value", request.form["song"])
                if "song" in request.form and request.form["song"]
                else None,
                ("field", "nested:artists.name")
                if "artist" in request.form and request.form["artist"]
                else None,
                ("value", request.form["artist"])
                if "artist" in request.form and request.form["artist"]
                else None,
                ("from", offset),
                ("size", pageSize),
                (
                    "videos_only",
                    "true"
                    if request.form.get("videos_only", "on") == "on"
                    else "false",
                ),
                *[
                    x
                    for y in [
                        [
                            ("field", "formats.name.keyword"),
                            ("value", v),
                        ]
                        for v in request.form.getlist("formats.name.keyword")
                    ]
                    for x in y
                ],
                *[
                    x
                    for y in [
                        [
                            ("field", "formats.descriptions.keyword"),
                            ("value", v),
                        ]
                        for v in request.form.getlist("formats.descriptions.keyword")
                    ]
                    for x in y
                ],
                *[
                    x
                    for y in [
                        [("field", "styles.keyword"), ("value", v)]
                        for v in request.form.getlist("styles.keyword")
                    ]
                    for x in y
                ],
            ]
            if p is not None
        ],
        timeout=10,
    )
    print(releases.url)
    releases.raise_for_status()
    releases = releases.json()

    hits = int(releases["hits"]["total"]["value"])

    releases = [r["_source"] for r in releases["hits"]["hits"]]

    for r in releases:
        if "videos" in r:
            r["videos"] = [v[32:] for v in r["videos"]]

    return render_template(
        "releases/hidden.jinja",
        **{
            "pageSize": pageSize,
            "releases": releases,
            "hits": hits,
            "page": page,
            "from": offset,
            "filters": filters.json(),
            **request.form,
        },
    )


@app.route("/thumb/<release_id>")
def thumb(release_id):
    if not "user" in session:
        raise Exception()

    return render_template(
        "image.jinja",
        src=(
            oauth.discogs.get(
                f"https://api.discogs.com/releases/{release_id}", timeout=5
            )
            .json()
            .get("thumb")
        ),
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


from sqlalchemy.dialects.postgresql import insert


@app.route("/auth")
def auth():
    token = oauth.discogs.authorize_access_token()
    resp = oauth.discogs.get("https://api.discogs.com/oauth/identity", timeout=5)
    user = resp.json()

    stmt = insert(User).values(
        discogs_oauth_token=token["oauth_token"],
        discogs_oauth_token_secret=token["oauth_token_secret"],
        discogs_user_id=user["id"],
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
