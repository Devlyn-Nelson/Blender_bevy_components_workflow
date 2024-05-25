import bpy
import os 
import subprocess
import json
import pytest
import shutil

import filecmp
from PIL import Image
from pixelmatch.contrib.PIL import pixelmatch

from blenvy.auto_export.export.prepare_and_export import prepare_and_export

@pytest.fixture
def setup_data(request):
    print("\nSetting up resources...")

    root_path =  "../../testing/bevy_example"
    assets_root_path = os.path.join(root_path, "assets")
    blueprints_path =  os.path.join(assets_root_path, "blueprints")
    levels_path =  os.path.join(assets_root_path, "levels")

    models_path =  os.path.join(assets_root_path, "models")
    materials_path = os.path.join(assets_root_path, "materials")
    yield {
        "root_path": root_path, 
        "models_path": models_path,
        "blueprints_path": blueprints_path, 
        "levels_path": levels_path, 
        "materials_path":materials_path 
        }

    def finalizer():
       
        #other_materials_path = os.path.join("../../testing", "other_materials")

        print("\nPerforming teardown...")
        if os.path.exists(blueprints_path):
            shutil.rmtree(blueprints_path)

        if os.path.exists(levels_path):
            shutil.rmtree(levels_path)

        if os.path.exists(models_path):
            shutil.rmtree(models_path)

        if os.path.exists(materials_path):
            shutil.rmtree(materials_path)

        diagnostics_file_path = os.path.join(root_path, "bevy_diagnostics.json")
        if os.path.exists(diagnostics_file_path):
            os.remove(diagnostics_file_path)
        
        hierarchy_file_path = os.path.join(root_path, "bevy_hierarchy.json")
        if os.path.exists(hierarchy_file_path):
            os.remove(hierarchy_file_path)

        screenshot_observed_path = os.path.join(root_path, "screenshot.png")
        if os.path.exists(screenshot_observed_path):
            os.remove(screenshot_observed_path)

    request.addfinalizer(finalizer)

    return None


"""
- calls exporter on the testing scene
- launches bevy app & checks for output
- checks screenshot, hierarchy & diagnostics files generated on the bevy side against reference files
- if all worked => test is a-ok
- removes generated files
"""
def test_export_complex(setup_data):
    root_path = setup_data["root_path"]
    auto_export_operator = bpy.ops.export_scenes.auto_gltf

    # with change detection
    # first, configure things
    # we use the global settings for that
    export_props = {
        "main_scene_names" : ['World'],
        "library_scene_names": ['Library'],
    }
    gltf_settings = {
        "export_animations": True,
        "export_optimize_animation_size": False
    }

    # store settings for the auto_export part
    stored_auto_settings = bpy.data.texts[".gltf_auto_export_settings"] if ".gltf_auto_export_settings" in bpy.data.texts else bpy.data.texts.new(".gltf_auto_export_settings")
    stored_auto_settings.clear()
    stored_auto_settings.write(json.dumps(export_props))

    # and store settings for the gltf part
    stored_gltf_settings = bpy.data.texts[".blenvy_gltf_settings"] if ".blenvy_gltf_settings" in bpy.data.texts else bpy.data.texts.new(".blenvy_gltf_settings")
    stored_gltf_settings.clear()
    stored_gltf_settings.write(json.dumps(gltf_settings))

    # move the main cube
    bpy.data.objects["Cube"].location = [1, 0, 0]
    # move the cube in the library
    # TODO: add back bpy.data.objects["Blueprint1_mesh"].location = [1, 2, 1]

    registry = bpy.context.window_manager.components_registry
    blenvy = bpy.context.window_manager.blenvy
    main_scene = blenvy.main_scenes.add()
    main_scene.name = "World"
    blenvy.auto_export.auto_export = True

    prepare_and_export()

    """auto_export_operator(
        auto_export=True,
        direct_mode=True,
        project_root_path = os.path.abspath(root_path),
        #blueprints_path = os.path.join("assets", "models", "library"),
        export_output_folder = os.path.join("assets", "models"), #"./models",
        #levels_path = os.path.join("assets", "models"),

        export_scene_settings=True,
        export_blueprints=True,
        export_materials_library=True
    )"""
    # blueprint1 => has an instance, got changed, should export
    # blueprint2 => has NO instance, but marked as asset, should export
    # blueprint3 => has NO instance, not marked as asset, used inside blueprint 4: should export
    # blueprint4 => has an instance, with nested blueprint3, should export
    # blueprint5 => has NO instance, not marked as asset, should NOT export

    assert os.path.exists(os.path.join(setup_data["levels_path"], "World.glb")) == True
    assert os.path.exists(os.path.join(setup_data["blueprints_path"], "Blueprint1.glb")) == True
    assert os.path.exists(os.path.join(setup_data["blueprints_path"], "Blueprint2.glb")) == True
    assert os.path.exists(os.path.join(setup_data["blueprints_path"], "Blueprint3.glb")) == True
    assert os.path.exists(os.path.join(setup_data["blueprints_path"], "Blueprint4_nested.glb")) == True
    assert os.path.exists(os.path.join(setup_data["blueprints_path"], "Blueprint5.glb")) == False
    assert os.path.exists(os.path.join(setup_data["blueprints_path"], "Blueprint6_animated.glb")) == True
    assert os.path.exists(os.path.join(setup_data["blueprints_path"], "Blueprint7_hierarchy.glb")) == True

    # 'assets_list_'+scene.name+"_components" should have been removed after the export
    assets_list_object_name = "assets_list_"+"World"+"_components"
    assets_list_object_present = assets_list_object_name in bpy.data.objects
    assert assets_list_object_present == False

    # now run bevy
    command = "cargo run --features bevy/dynamic_linking"
    FNULL = open(os.devnull, 'w')    #use this if you want to suppress output to stdout from the subprocess
    return_code = subprocess.call(["cargo", "run", "--features", "bevy/dynamic_linking"], cwd=root_path)
    print("RETURN CODE OF BEVY APP", return_code)
    assert return_code == 0

    with open(os.path.join(root_path, "bevy_diagnostics.json")) as diagnostics_file:
        diagnostics = json.load(diagnostics_file)
        print("diagnostics", diagnostics)
        assert diagnostics["animations"] == True
        assert diagnostics["empty_found"] == True
        assert diagnostics["blueprints_list_found"] == True
        assert diagnostics["exported_names_correct"] == True

    with open(os.path.join(root_path, "bevy_hierarchy.json")) as hierarchy_file:
        with open(os.path.join(os.path.dirname(__file__), "expected_bevy_hierarchy.json")) as expexted_hierarchy_file:
            hierarchy = json.load(hierarchy_file)
            expected = json.load(expexted_hierarchy_file)
            assert sorted(hierarchy.items()) == sorted(expected.items())

    # last but not least, do a visual compare
    screenshot_expected_path = os.path.join(os.path.dirname(__file__), "expected_screenshot.png")
    screenshot_observed_path = os.path.join(root_path, "screenshot.png")
    img_a = Image.open(screenshot_expected_path)
    img_b = Image.open(screenshot_observed_path)
    img_diff = Image.new("RGBA", img_a.size)
    mismatch = pixelmatch(img_a, img_b, img_diff, includeAA=True)
    print("image mismatch", mismatch)
    assert mismatch < 50


        
