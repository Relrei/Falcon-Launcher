# Falcon Launcher とレンダリングの一覧をつなぐアドオン(どの Blender にも入れられる単独の 1 ファイル)。
# 開いている Blender のシーンでチェックを入れるとランチャーに登録され、消すのはランチャーか開いているシーンで。
#
# シーンの「ランチャーに登録」を入れると、そのシーンの写し(描くのは写し)を
#   <届け先>/<id>/scene.blend に書き、範囲などを <届け先>/<id>/entry.tsv に書く。ランチャーはそれを一覧に足す。
# チェックを外すとフォルダを消す(ランチャーは一覧から外す)。ランチャーで消されてフォルダが無くなったら、チェックを外す。
# 範囲などを変えると entry.tsv を書き直す。写しはボタンで取り直す。
#
# 届け先は ~/.config/falcon-launcher/連携.tsv の「inbox<TAB>パス」。無ければ ~/.local/share/falcon-launcher/届いた。
# SPDX-License-Identifier: GPL-2.0-or-later

bl_info = {
    "name": "Falcon Launcher Link",
    "author": "Relrei",
    "version": (0, 2, 0),
    "blender": (2, 93, 0),
    "location": "プロパティ > 出力 > Falcon Launcher",
    "description": "シーンにチェックを入れると、Falcon Launcher のレンダリングの一覧に登録する",
    "category": "Render",
}

import os
import shutil
import time
import random

import bpy
from bpy.app.handlers import persistent
from bpy.props import BoolProperty, IntProperty, StringProperty, PointerProperty

# チェックを自分で外す時に、update が写しを消しに行かないための印
_quiet = set()


def inbox_dir():
    cfg = os.path.join(os.path.expanduser("~"), ".config", "falcon-launcher", "連携.tsv")
    try:
        with open(cfg, encoding="utf-8") as f:
            for line in f:
                k, _, v = line.rstrip("\n").partition("\t")
                if k == "inbox" and v:
                    return v
    except OSError:
        pass
    return os.path.join(os.path.expanduser("~"), ".local", "share", "falcon-launcher", "届いた")


def clean(s):
    return str(s).replace("\t", " ").replace("\n", " ")


def entry_dir(link):
    return os.path.join(inbox_dir(), link.entry_id) if link.entry_id else ""


def write_entry(scene):
    """範囲などを entry.tsv に書く(書き終えてから名前を変える)"""
    link = scene.falcon_link
    d = entry_dir(link)
    if not d or not os.path.isdir(d):
        return
    if link.use_scene_range:
        a, b, s = scene.frame_start, scene.frame_end, scene.frame_step
    else:
        a, b, s = link.frame_start, link.frame_end, link.frame_step
    lines = [
        f"source\t{clean(bpy.data.filepath)}",
        f"binary\t{clean(bpy.app.binary_path)}",
        f"version\t{bpy.app.version_string}",
        f"scene\t{clean(scene.name)}",
        f"start\t{a}",
        f"end\t{b}",
        f"step\t{max(s, 1)}",
        f"camera\t{clean(link.camera)}",
        f"keep_output\t{int(link.keep_output)}",
        f"updated\t{int(time.time())}",
    ]
    tmp = os.path.join(d, ".entry.tsv")
    with open(tmp, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    os.replace(tmp, os.path.join(d, "entry.tsv"))


def write_snapshot(scene):
    """シーンと、それが使う物だけを写しに書く。パスは絶対にして、写しの場所でも絵などが見つかるようにする"""
    link = scene.falcon_link
    d = entry_dir(link)
    os.makedirs(d, exist_ok=True)
    tmp = os.path.join(d, ".scene.blend")
    bpy.data.libraries.write(tmp, {scene}, path_remap="ABSOLUTE", fake_user=False)
    os.replace(tmp, os.path.join(d, "scene.blend"))


def register_scene(scene):
    link = scene.falcon_link
    if not link.entry_id:
        link.entry_id = time.strftime("%Y%m%d-%H%M%S") + f"-{random.randint(0, 9999):04d}"
    write_snapshot(scene)
    write_entry(scene)


def unregister_scene(scene):
    d = entry_dir(scene.falcon_link)
    # ランチャーの届け先の中の、このアドオンが作ったフォルダだけを消す
    if d and os.path.isfile(os.path.join(d, "entry.tsv")):
        shutil.rmtree(d, ignore_errors=True)
    scene.falcon_link.entry_id = ""


def on_toggle(self, context):
    scene = self.id_data
    if scene.name in _quiet:
        return
    try:
        if self.on:
            register_scene(scene)
        else:
            unregister_scene(scene)
    except Exception as e:  # 書けない時はチェックを戻す
        print("Falcon Launcher Link:", e)


def on_change(self, context):
    if self.on:
        try:
            write_entry(self.id_data)
        except Exception as e:
            print("Falcon Launcher Link:", e)


class FALCONLINK_Scene(bpy.types.PropertyGroup):
    on: BoolProperty(name="ランチャーに登録", description="入れると、このシーンの写しを Falcon Launcher のレンダリングの一覧に登録する。外すと一覧から外す", default=False, update=on_toggle)
    entry_id: StringProperty()
    use_scene_range: BoolProperty(name="シーンの範囲", default=True, update=on_change)
    frame_start: IntProperty(name="始め", default=1, update=on_change)
    frame_end: IntProperty(name="終わり", default=250, update=on_change)
    frame_step: IntProperty(name="間隔", default=1, min=1, update=on_change)
    camera: StringProperty(name="カメラ", description="空ならシーンのカメラ", update=on_change)
    keep_output: BoolProperty(name="シーンの出力設定のまま", description="切ると、ランチャーの置き場へ PNG で書く", default=False, update=on_change)


class FALCONLINK_OT_resnap(bpy.types.Operator):
    bl_idname = "falcon_link.resnap"
    bl_label = "写しを取り直す"
    bl_description = "今のシーンの状態で、ランチャーが描く写しを書き直す(描いてある枚は残り、続きから今の状態で描く)"

    def execute(self, context):
        sc = context.scene
        if not sc.falcon_link.on:
            return {"CANCELLED"}
        register_scene(sc)
        self.report({"INFO"}, "写しを取り直しました")
        return {"FINISHED"}


class FALCONLINK_PT_panel(bpy.types.Panel):
    bl_label = "Falcon Launcher"
    bl_space_type = "PROPERTIES"
    bl_region_type = "WINDOW"
    bl_context = "output"

    def draw_header(self, context):
        self.layout.prop(context.scene.falcon_link, "on", text="")

    def draw(self, context):
        layout = self.layout
        sc = context.scene
        link = sc.falcon_link
        col = layout.column()
        col.prop(link, "on")
        body = col.column()
        body.active = link.on
        body.prop(link, "use_scene_range")
        if link.use_scene_range:
            body.label(text=f"{sc.frame_start}–{sc.frame_end}({sc.frame_step} おき)")
        else:
            r = body.row(align=True)
            r.prop(link, "frame_start")
            r.prop(link, "frame_end")
            r.prop(link, "frame_step")
        body.prop_search(link, "camera", bpy.data, "objects", text="カメラ", icon="CAMERA_DATA")
        body.prop(link, "keep_output")
        if link.on:
            body.operator("falcon_link.resnap", icon="FILE_REFRESH")
            body.label(text="登録済み · 描くのはチェックを入れた時の写しです", icon="CHECKMARK")
        layout.label(text=f"届け先: {inbox_dir()}")


def check_removed():
    """ランチャーで消された物(フォルダが無くなった物)のチェックを外す"""
    try:
        for sc in bpy.data.scenes:
            link = sc.falcon_link
            if link.on and link.entry_id and not os.path.isdir(entry_dir(link)):
                _quiet.add(sc.name)
                try:
                    link.on = False
                    link.entry_id = ""
                finally:
                    _quiet.discard(sc.name)
    except Exception as e:
        print("Falcon Launcher Link:", e)
    return 2.0


@persistent
def on_load(_):
    check_removed()


classes = (FALCONLINK_Scene, FALCONLINK_OT_resnap, FALCONLINK_PT_panel)


def register():
    for c in classes:
        bpy.utils.register_class(c)
    bpy.types.Scene.falcon_link = PointerProperty(type=FALCONLINK_Scene)
    bpy.app.handlers.load_post.append(on_load)
    bpy.app.timers.register(check_removed, first_interval=2.0, persistent=True)


def unregister():
    if bpy.app.timers.is_registered(check_removed):
        bpy.app.timers.unregister(check_removed)
    if on_load in bpy.app.handlers.load_post:
        bpy.app.handlers.load_post.remove(on_load)
    del bpy.types.Scene.falcon_link
    for c in reversed(classes):
        bpy.utils.unregister_class(c)


if __name__ == "__main__":
    register()
