"""Create an isolated server document for smoke.txt; never modify default."""
import json
import os
import uuid
import urllib.request

url = os.environ["RUTABLE_API_URL"].rstrip("/")
token = os.environ["RUTABLE_API_TOKEN"]
document_id = "api-test-" + uuid.uuid4().hex[:12]
fields = [
    {"id": "f" + str(i), "name": name, "kind": {"type": "Text"}, "description": ""}
    for i, name in enumerate(["标题", "字段一", "字段二", "字段三", "备注"])
]
document = {
    "name": "Rutable API 验证",
    "me": "self",
    "members": [{"id": "self", "name": "我", "color": 0}],
    "order": ["smokeTable"],
    "tables": [{
        "id": "smokeTable", "name": "API 测试", "fields": fields, "seq": 1,
        "records": [{"id": "smokeRecord", "cells": {"f0": {"t": "Text", "v": "接口测试"}},
                     "created_at": 0, "updated_at": 0, "seq": 1}],
        "views": [{"id": "smokeView", "name": "表格", "kind": "Grid"}],
    }],
}
body = json.dumps({"expected_revision": 0, "document": document}, ensure_ascii=False).encode()
request = urllib.request.Request(
    url + "/api/v1/documents/" + document_id, data=body, method="PUT",
    headers={"Content-Type": "application/json", "Authorization": "Bearer " + token},
)
with urllib.request.urlopen(request, timeout=20) as response:
    assert response.status == 200
print(json.dumps({"api": url, "document_id": document_id}, ensure_ascii=False))
