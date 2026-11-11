#!/usr/bin/env python3
"""Create fake Chrome and Firefox profiles below HOME_DIR for importer e2e tests.

usage: import-fixtures.py HOME_DIR
Same minimal schemas as core/src/importer.rs tests. Prints the expected counts.
"""
import json, os, sqlite3, sys, time

home = sys.argv[1]
now_ms = int(time.time() * 1000)
DAY = 86400000
webkit = lambda ms: (ms + 11644473600000) * 1000

chrome = os.path.join(home, ".config/google-chrome")
os.makedirs(os.path.join(chrome, "Default"), exist_ok=True)
json.dump({"profile": {"info_cache": {"Default": {"name": "Person 1"}}}}, open(os.path.join(chrome, "Local State"), "w"))
json.dump({"roots": {
    "bookmark_bar": {"name": "Bookmarks bar", "type": "folder", "children": [
        {"type": "url", "name": "Rust", "url": "https://www.rust-lang.org/"},
        {"type": "folder", "name": "News", "children": [
            {"type": "url", "name": "HN", "url": "https://news.ycombinator.com/"},
            {"type": "url", "name": "Bad", "url": "javascript:alert(1)"}]}]},
    "other": {"name": "Other bookmarks", "type": "folder", "children": [
        {"type": "url", "name": "Ex", "url": "http://example.com/a"}]},
    "synced": {"name": "Mobile bookmarks", "type": "folder", "children": []}}},
    open(os.path.join(chrome, "Default/Bookmarks"), "w"))
db = os.path.join(chrome, "Default/History")
if os.path.exists(db): os.remove(db)
c = sqlite3.connect(db)
c.execute("CREATE TABLE urls (id INTEGER PRIMARY KEY, url LONGVARCHAR, title LONGVARCHAR, visit_count INTEGER DEFAULT 0 NOT NULL, last_visit_time INTEGER NOT NULL)")
for u, t, ms in [("https://a.example/", "A", now_ms - 60000), ("https://b.example/", "B", now_ms - 120000),
                 ("https://old.example/", "Old", now_ms - 400 * DAY), ("chrome://version/", "V", now_ms - 3000)]:
    c.execute("INSERT INTO urls (url,title,visit_count,last_visit_time) VALUES (?,?,1,?)", (u, t, webkit(ms)))
c.commit(); c.close()

ff = os.path.join(home, ".mozilla/firefox")
os.makedirs(os.path.join(ff, "abc.default-release"), exist_ok=True)
open(os.path.join(ff, "profiles.ini"), "w").write("[Profile0]\nName=default-release\nIsRelative=1\nPath=abc.default-release\nDefault=1\n\n[General]\nVersion=2\n")
db = os.path.join(ff, "abc.default-release/places.sqlite")
if os.path.exists(db): os.remove(db)
c = sqlite3.connect(db)
c.executescript("""
CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url LONGVARCHAR, title LONGVARCHAR, visit_count INTEGER DEFAULT 0, hidden INTEGER DEFAULT 0 NOT NULL, last_visit_date INTEGER);
CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER DEFAULT NULL, parent INTEGER, position INTEGER, title LONGVARCHAR, guid TEXT);
""")
for i, (u, t, vc, h, ms) in enumerate([("https://mozilla.org/", "Mozilla", 3, 0, now_ms - 180000), ("https://hidden.example/", "Hidden", 1, 1, now_ms - 1000),
        ("place:type=6", "Recent", 0, 0, None), ("https://fx.example/", "FX", 1, 0, now_ms - 240000)], 1):
    c.execute("INSERT INTO moz_places VALUES (?,?,?,?,?,?)", (i, u, t, vc, h, None if ms is None else ms * 1000))
c.executescript("""
INSERT INTO moz_bookmarks VALUES (1,2,NULL,0,0,'','root________'),(2,2,NULL,1,0,'menu','menu________'),
 (3,2,NULL,1,1,'Bookmarks Toolbar','toolbar_____'),(4,2,NULL,1,2,'Other','unfiled_____'),
 (7,1,1,3,0,'Mozilla','g7'),(8,2,NULL,3,1,'Dev','g8'),(9,1,4,8,0,'FX','g9'),(10,1,3,4,0,'Query','g10');
""")
c.commit(); c.close()
print("expected first import: bookmarks 3+2=5 (+folders), history 2+2=4")
