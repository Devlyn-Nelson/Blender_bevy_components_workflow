use std::path::{Path, PathBuf};

use bevy::{asset::LoadedUntypedAsset, ecs::entity, gltf::Gltf, prelude::*, render::view::visibility, scene::SceneInstance, transform::commands, utils::hashbrown::HashMap};
use serde_json::Value;

use crate::{BlueprintAssets, BlueprintAssetsLoadState, AssetLoadTracker, BlenvyConfig, BlueprintAnimations, BlueprintAssetsLoaded, BlueprintAssetsNotLoaded};

/// this is a flag component for our levels/game world
#[derive(Component)]
pub struct GameWorldTag;

/// Main component for the blueprints
/// has both name & path of the blueprint to enable injecting the data from the correct blueprint
/// into the entity that contains this component 
#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
pub struct BlueprintInfo {
    pub name: String,
    pub path: String,
}

/// flag component needed to signify the intent to spawn a Blueprint
#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
pub struct SpawnHere;

#[derive(Component)]
/// flag component for dynamically spawned scenes
pub struct Spawned;


#[derive(Component, Debug)]
/// flag component added when a Blueprint instance ist Ready : ie : 
/// - its assets have loaded
/// - it has finished spawning
pub struct BlueprintInstanceReady;

#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
/// flag component marking any spwaned child of blueprints ..unless the original entity was marked with the `NoInBlueprint` marker component
pub struct InBlueprint;

#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
/// flag component preventing any spawned child of blueprints to be marked with the `InBlueprint` component
pub struct NoInBlueprint;

#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
// this allows overriding the default library path for a given entity/blueprint
pub struct Library(pub PathBuf);

#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
/// flag component to force adding newly spawned entity as child of game world
pub struct AddToGameWorld;

#[derive(Component)]
/// helper component, just to transfer child data
pub(crate) struct OriginalChildren(pub Vec<Entity>);


#[derive(Component)]
/// You can add this component to a blueprint instance, and the instance will be hidden until it is ready 
/// You usually want to use this for worlds/level spawning , or dynamic spawning at runtime, but not when you are adding blueprint instances to an existing entity
/// as it would first become invisible before re-appearing again
pub struct HideUntilReady;

#[derive(Event, Debug)]
pub enum BlueprintEvent {

    /// event fired when a blueprint has finished loading its assets & before it attempts spawning
    AssetsLoaded {
        entity: Entity,
        blueprint_name: String,
        blueprint_path: String,
        // TODO: add assets list ?
    },
    /// event fired when a blueprint is COMPLETELY done spawning ie
    /// - all its assets have been loaded
    /// - the spawning attempt has been sucessfull
    Spawned {
        entity: Entity,
        blueprint_name: String,
        blueprint_path: String,
    },

    /// 
    InstanceReady {
        entity: Entity,
        blueprint_name: String,
        blueprint_path: String,
    }
    
}


// TODO: move this somewhere else ?
#[derive(Component, Reflect, Debug, Default)]
#[reflect(Component)]
/// component used to mark any entity as Dynamic: aka add this to make sure your entity is going to be saved
pub struct DynamicBlueprintInstance;


// TODO: move these somewhere else ?
#[derive(Component, Reflect, Debug, Default)]
#[reflect(Component)]
/// component gets added when a blueprint starts spawning, removed when spawning is done
pub struct BlueprintSpawning;


use gltf::Gltf as RawGltf;

pub(crate) fn blueprints_prepare_spawn(
    blueprint_instances_to_spawn : Query<
    (
        Entity,
        &BlueprintInfo,
        Option<&Parent>,
        Option<&BlueprintAssets>,
    ),(Added<SpawnHere>)
    >,
mut commands: Commands,
asset_server: Res<AssetServer>,
) {
   
    for (entity, blueprint_info, parent, all_assets) in blueprint_instances_to_spawn.iter() {
        info!("BLUEPRINT: to spawn detected: {:?} path:{:?}", blueprint_info.name, blueprint_info.path);
        //println!("all assets {:?}", all_assets);
        //////////////

        // we add the asset of the blueprint itself
        // TODO: add detection of already loaded data
        let untyped_handle = asset_server.load_untyped(&blueprint_info.path);
        let asset_id = untyped_handle.id();
        let loaded = asset_server.is_loaded_with_dependencies(asset_id);

        let mut asset_infos: Vec<AssetLoadTracker> = vec![];
        if !loaded {
            asset_infos.push(AssetLoadTracker {
                name: blueprint_info.name.clone(),
                path: blueprint_info.path.clone(),
                id: asset_id,
                loaded: false,
                handle: untyped_handle.clone(),
            });
        }

        // and we also add all its assets
        /* prefetch attempt */
        let gltf = RawGltf::open(format!("assets/{}", blueprint_info.path)).unwrap();
        for scene in gltf.scenes() {
            let foo_extras = scene.extras().clone().unwrap();

            let lookup: HashMap<String, Value> = serde_json::from_str(&foo_extras.get()).unwrap();
            /*for (key, value) in lookup.clone().into_iter() {
                println!("{} / {}", key, value);
            }*/

            if lookup.contains_key("BlueprintAssets"){
                let assets_raw = &lookup["BlueprintAssets"];
                //println!("ASSETS RAW {}", assets_raw);
                let all_assets: BlueprintAssets = ron::from_str(&assets_raw.as_str().unwrap()).unwrap();
                // println!("all_assets {:?}", all_assets);

                for asset in all_assets.assets.iter() {
                    let untyped_handle = asset_server.load_untyped(&asset.path);
                    //println!("untyped handle {:?}", untyped_handle);
                    //asset_server.load(asset.path);
                    let asset_id = untyped_handle.id();
                    //println!("ID {:?}", asset_id);
                    let loaded = asset_server.is_loaded_with_dependencies(asset_id);
                    //println!("Loaded ? {:?}", loaded);
                    if !loaded {
                        asset_infos.push(AssetLoadTracker {
                            name: asset.name.clone(),
                            path: asset.path.clone(),
                            id: asset_id,
                            loaded: false,
                            handle: untyped_handle.clone(),
                        });
                    }
                }
            }
        }

        // now insert load tracker
        if !asset_infos.is_empty() {
            commands
                .entity(entity)
                .insert(BlueprintAssetsLoadState {
                    all_loaded: false,
                    asset_infos,
                    ..Default::default()
                })
                .insert(BlueprintAssetsNotLoaded)                
                ;
        } else {
            commands.entity(entity).insert(BlueprintAssetsLoaded);
        }


        commands.entity(entity).insert(BlueprintSpawning);
    }
}

pub(crate) fn blueprints_check_assets_loading(
    mut blueprint_assets_to_load: Query<
        (Entity, Option<&Name>, &BlueprintInfo, &mut BlueprintAssetsLoadState),
        With<BlueprintAssetsNotLoaded>,
    >,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    mut blueprint_events: EventWriter<BlueprintEvent>,

) {
    for (entity, entity_name, blueprint_info, mut assets_to_load) in blueprint_assets_to_load.iter_mut() {
        let mut all_loaded = true;
        let mut loaded_amount = 0;
        let total = assets_to_load.asset_infos.len();
        for tracker in assets_to_load.asset_infos.iter_mut() {
            let asset_id = tracker.id;
            let loaded = asset_server.is_loaded_with_dependencies(asset_id);
            // println!("loading {}: // load state: {:?}", tracker.name, asset_server.load_state(asset_id));

            // FIXME: hack for now
            let mut failed = false;// asset_server.load_state(asset_id) == bevy::asset::LoadState::Failed(_error);
            match asset_server.load_state(asset_id) {
                bevy::asset::LoadState::Failed(_) => {
                    failed = true
                },
                _ => {}
            }
            tracker.loaded = loaded || failed;
            if loaded || failed {
                loaded_amount += 1;
            } else {
                all_loaded = false;
            }
        }
        let progress: f32 = loaded_amount as f32 / total as f32;
        assets_to_load.progress = progress;

        if all_loaded {
            assets_to_load.all_loaded = true;
            // println!("LOADING: DONE for ALL assets of {:?} (instance of {}), preparing for spawn", entity_name, blueprint_info.path);
            // blueprint_events.send(BlueprintEvent::AssetsLoaded {blueprint_name:"".into(), blueprint_path: blueprint_info.path.clone() });

            commands
                .entity(entity)
                .insert(BlueprintAssetsLoaded)
                .remove::<BlueprintAssetsNotLoaded>()
                //.remove::<BlueprintAssetsLoadState>() //REMOVE it in release mode/ when hot reload is off, keep it for dev/hot reload
                ;
        }else {
            // println!("LOADING: in progress for ALL assets of {:?} (instance of {}): {} ",entity_name, blueprint_info.path, progress * 100.0);
        }
    }
}


pub(crate) fn blueprints_assets_ready(spawn_placeholders: Query<
    (
        Entity,
        &BlueprintInfo,
        Option<&Transform>,
        Option<&Parent>,
        Option<&AddToGameWorld>,
        Option<&Name>,
        Option<&HideUntilReady>
    ),
    (
        With<BlueprintAssetsLoaded>,
        Added<BlueprintAssetsLoaded>,
        Without<BlueprintAssetsNotLoaded>,
    ),
>,

    mut commands: Commands,
    mut game_world: Query<Entity, With<GameWorldTag>>,

    assets_gltf: Res<Assets<Gltf>>,
    asset_server: Res<AssetServer>,
    children: Query<&Children>,)
{
    for (
        entity,
        blueprint_info,
        transform,
        original_parent,
        add_to_world,
        name,
        hide_until_ready,
    ) in spawn_placeholders.iter()
    {
        /*info!(
            "BLUEPRINT: all assets loaded, attempting to spawn blueprint SCENE {:?} for entity {:?}, id: {:}, parent:{:?}",
            blueprint_info.name, name, entity, original_parent
        );*/

        info!(
            "BLUEPRINT: all assets loaded, attempting to spawn blueprint SCENE {:?} for entity {:?}, id: {}",
            blueprint_info.name, name, entity
        );

        // info!("attempting to spawn {:?}", model_path);
        let model_handle: Handle<Gltf> = asset_server.load(blueprint_info.path.clone()); // FIXME: kinda weird now

        let gltf = assets_gltf.get(&model_handle).unwrap_or_else(|| {
            panic!(
                "gltf file {:?} should have been loaded",
                &blueprint_info.path
            )
        });

        // WARNING we work under the assumtion that there is ONLY ONE named scene, and that the first one is the right one
        let main_scene_name = gltf
            .named_scenes
            .keys()
            .next()
            .expect("there should be at least one named scene in the gltf file to spawn");

        let scene = &gltf.named_scenes[main_scene_name];

        // transforms are optional, but still deal with them correctly
        let mut transforms: Transform = Transform::default();
        if transform.is_some() {
            transforms = *transform.unwrap();
        }

        let mut original_children: Vec<Entity> = vec![];
        if let Ok(c) = children.get(entity) {
            for child in c.iter() {
                original_children.push(*child);
            }
        }

        let mut named_animations:HashMap<String, Handle<AnimationClip>> = HashMap::new() ;
        for (key, value) in gltf.named_animations.iter() {
            named_animations.insert(key.to_string(), value.clone());
        }

        commands.entity(entity).insert((
            SceneBundle {
                scene: scene.clone(),
                transform: transforms,
                ..Default::default()
            },
            OriginalChildren(original_children),
            BlueprintAnimations {
                // these are animations specific to the inside of the blueprint
                named_animations: named_animations//gltf.named_animations.clone(),
            },
        ));

        if hide_until_ready.is_some() {
            commands.entity(entity).insert(Visibility::Hidden); // visibility: 
        }

        // only allow automatically adding a newly spawned blueprint instance to the "world", if the entity does not have a parent 
         if add_to_world.is_some() && original_parent.is_some() {
            let world = game_world
                .get_single_mut()
                .expect("there should be a game world present");
            commands.entity(world).add_child(entity);
        } 

    }
}


#[derive(Component, Reflect, Debug, Default)]
#[reflect(Component)]
pub struct SubBlueprintsSpawnTracker{
    pub sub_blueprint_instances: HashMap<Entity, bool>
}

#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub struct SpawnTrackRoot(pub Entity);

#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub struct BlueprintSceneSpawned;


#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub struct BlueprintChildrenReady;

#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub struct BlueprintReadyForPostProcess;

pub(crate) fn blueprints_scenes_spawned(
    spawned_blueprint_scene_instances: Query<(Entity, Option<&Name>, Option<&Children>, Option<&SpawnTrackRoot>), (With<BlueprintSpawning>, Added<SceneInstance>)>,
    with_blueprint_infos : Query<(Entity, Option<&Name>), With<BlueprintInfo>>,

    all_children: Query<&Children>,
    all_parents: Query<&Parent>,

    mut sub_blueprint_trackers: Query<(Entity, &mut SubBlueprintsSpawnTracker, &BlueprintInfo)>,

    mut commands: Commands,

    all_names: Query<&Name>
) {
    for (entity, name, children, track_root) in spawned_blueprint_scene_instances.iter(){
        info!("Done spawning blueprint scene for entity named {:?} (track root: {:?})", name, track_root);
        let mut sub_blueprint_instances: Vec<Entity> = vec![];
        let mut sub_blueprint_instance_names: Vec<Name> = vec![];

        let mut tracker_data: HashMap<Entity, bool> = HashMap::new();
        
        if track_root.is_none() {
            for parent in all_parents.iter_ancestors(entity) {
                if with_blueprint_infos.get(parent).is_ok() {
    
                    println!("found a parent with blueprint_info {:?} for {:?}", all_names.get(parent), all_names.get(entity));
                    commands.entity(entity).insert(SpawnTrackRoot(parent));// Injecting to know which entity is the root

                    break;
                }
            }
        }
    

        if children.is_some() {
            for child in all_children.iter_descendants(entity) {
                if with_blueprint_infos.get(child).is_ok() {
                    // println!("Parent blueprint instance of {:?} is {:?}",  all_names.get(child), all_names.get(entity));


                   

                    for parent in all_parents.iter_ancestors(child) {
                        if with_blueprint_infos.get(parent).is_ok() {
            
                            if parent == entity {
                                //println!("yohoho");
                                println!("Parent blueprint instance of {:?} is {:?}",  all_names.get(child), all_names.get(parent));

                                commands.entity(child).insert(SpawnTrackRoot(entity));// Injecting to know which entity is the root

                                tracker_data.insert(child, false);

                                sub_blueprint_instances.push(child);
                                if let Ok(nname) = all_names.get(child) {
                                    sub_blueprint_instance_names.push(nname.clone());
                                }

                                /*if track_root.is_some() {
                                    let prev_root = track_root.unwrap().0;
                                    // if we already had a track root, and it is different from the current entity , change the previous track root's list of children
                                    if prev_root != entity {
                                        let mut tracker = sub_blueprint_trackers.get_mut(prev_root).expect("should have a tracker");
                                        tracker.1.sub_blueprint_instances.remove(&child);
                                    }
                                }*/

                            }
                            break;
                        }
                    }


            
                }
            }
        }
       


        println!("sub blueprint instances {:?}", sub_blueprint_instance_names);
        
        // TODO: how about when no sub blueprints are present
        if tracker_data.keys().len() > 0 {
            commands.entity(entity)
                .insert(SubBlueprintsSpawnTracker{sub_blueprint_instances: tracker_data.clone()});
        }else {
            commands.entity(entity).insert(BlueprintChildrenReady);    
        }
    }
}

// could be done differently, by notifying each parent of a spawning blueprint that this child is done spawning ?
// perhaps using component hooks or observers (ie , if a ComponentSpawning + Parent)

use crate:: CopyComponents;
use std::any::TypeId;


/// this system is in charge of doing component transfers & co
/// - it removes one level of useless nesting
/// - it copies the blueprint's root components to the entity it was spawned on (original entity)
/// - it copies the children of the blueprint scene into the original entity
/// - it add `AnimationLink` components so that animations can be controlled from the original entity
pub(crate) fn blueprints_transfer_components(
    foo: Query<(
        Entity, 
        &Children,
        &OriginalChildren,
        Option<&Name>, 
        Option<&SpawnTrackRoot>), 
        Added<BlueprintChildrenReady>
    >,
    mut sub_blueprint_trackers: Query<(Entity, &mut SubBlueprintsSpawnTracker, &BlueprintInfo)>,
    all_children: Query<&Children>,

    mut commands: Commands,

    all_names: Query<&Name>
) {

    for (original, children, original_children, name, track_root) in foo.iter() {
        info!("YOOO ready !! removing empty nodes {:?}", name);

        if children.len() == 0 {
            warn!("timing issue ! no children found, please restart your bevy app (bug being investigated)");
            continue;
        }
        // the root node is the first & normally only child inside a scene, it is the one that has all relevant components
        let mut blueprint_root_entity = Entity::PLACEHOLDER; //FIXME: and what about childless ones ?? => should not be possible normally
                                                   // let diff = HashSet::from_iter(original_children.0).difference(HashSet::from_iter(children));
                                                   // we find the first child that was not in the entity before (aka added during the scene spawning)
        for child in children.iter() {
            if !original_children.0.contains(child) {
                blueprint_root_entity = *child;
                break;
            }
        }

        // copy components into from blueprint instance's blueprint_root_entity to original entity
        commands.add(CopyComponents {
            source: blueprint_root_entity,
            destination: original,
            exclude: vec![TypeId::of::<Parent>(), TypeId::of::<Children>()],
            stringent: false,
        });

        // we move all of children of the blueprint instance one level to the original entity to avoid having an additional, useless nesting level
        if let Ok(root_entity_children) = all_children.get(blueprint_root_entity) {
            for child in root_entity_children.iter() {
                // info!("copying child {:?} upward from {:?} to {:?}", names.get(*child), blueprint_root_entity, original);
                commands.entity(original).add_child(*child);
            }
        }

        commands.entity(original)
            .insert(BlueprintReadyForPostProcess); // Tag the entity so any systems dealing with post processing can know it is now their "turn" 
        // commands.entity(original).remove::<Handle<Scene>>(); // FIXME: if we delete the handle to the scene, things get despawned ! not what we want
        //commands.entity(original).remove::<BlueprintAssetsLoadState>(); // also clear the sub assets tracker to free up handles, perhaps just freeing up the handles and leave the rest would be better ?
        //commands.entity(original).remove::<BlueprintAssetsLoaded>();
        commands.entity(blueprint_root_entity).despawn_recursive(); // Remove the root entity that comes from the spawned-in scene


        // now check if the current entity is a child blueprint instance of another entity
        // this should always be done last, as children should be finished before the parent can be processed correctly
        // TODO: perhaps use observers for these
        if let Some(track_root) = track_root {
            let root_name = all_names.get(track_root.0);
            println!("got some root {:?}", root_name);
            if let Ok((s_entity, mut tracker, bp_info)) = sub_blueprint_trackers.get_mut(track_root.0) {
                tracker.sub_blueprint_instances.entry(original).or_insert(true);
                tracker.sub_blueprint_instances.insert(original, true);

                // TODO: ugh, my limited rust knowledge, this is bad code
                let mut all_spawned = true;
                for val in tracker.sub_blueprint_instances.values() {
                    if !val {
                        all_spawned = false;
                        break;
                    }
                }
                if all_spawned {
                    // println!("ALLLLL SPAAAAWNED for {} named {:?}", track_root.0, root_name);
                    commands.entity(track_root.0).insert(BlueprintChildrenReady);
                } 
            }
        } 
    }
}



#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub struct BlueprintReadyForFinalizing;

pub(crate) fn blueprints_finalize_instances(
    blueprint_instances: Query<(Entity, Option<&Name>, &BlueprintInfo, Option<&HideUntilReady>), (With<BlueprintSpawning>, With<BlueprintReadyForFinalizing>)>,
    mut blueprint_events: EventWriter<BlueprintEvent>,
    mut commands: Commands,
) {
    for (entity, name, blueprint_info, hide_until_ready) in blueprint_instances.iter() {
        info!("Finalizing blueprint instance {:?}", name);
        commands.entity(entity)
            .remove::<SpawnHere>()
            .remove::<BlueprintSpawning>()
            .remove::<BlueprintReadyForPostProcess>()
            .insert(BlueprintInstanceReady)
            ;
        if hide_until_ready.is_some() {
            println!("REVEAAAL");
            commands.entity(entity).insert(Visibility::Visible);
        }


        blueprint_events.send(BlueprintEvent::InstanceReady {entity: entity, blueprint_name: blueprint_info.name.clone(), blueprint_path: blueprint_info.path.clone()});
    }
}
/*
=> annoying issue with the "nested" useless root node created by blender
            => distinguish between blueprint instances inside blueprint instances vs blueprint instances inside blueprints ??

BlueprintSpawning
    - Blueprint Load Assets
    - Blueprint Assets Ready: spawn Blueprint's scene
    - Blueprint Scene Ready (SceneInstance component is present):
        - get list of sub Blueprints if any, inject sub blueprints spawn tracker
    - Blueprint copy components to original entity, remove useless nodes
    - Blueprint post process
        - generate aabb (need full hierarchy in its final form)
        - inject materials from library if needed
    - Blueprint Ready 
        - bubble information up to parent blueprint instance
        - if all sub_blueprints are ready => Parent blueprint Instance is ready 
*/


// HOT RELOAD


use bevy::asset::AssetEvent;

pub(crate) fn react_to_asset_changes(
    mut gltf_events: EventReader<AssetEvent<Gltf>>,
    mut untyped_events: EventReader<AssetEvent<LoadedUntypedAsset>>,
    mut blueprint_assets: Query<(Entity, Option<&Name>, &BlueprintInfo, &mut BlueprintAssetsLoadState, Option<&Children>)>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,

) {

    for event in gltf_events.read() {
        // LoadedUntypedAsset
        match event {
            AssetEvent::Modified { id } => {
                // React to the image being modified
                // println!("Modified gltf {:?}", asset_server.get_path(*id));
                for (entity, entity_name, blueprint_info, mut assets_to_load, children) in blueprint_assets.iter_mut() {
                    for tracker in assets_to_load.asset_infos.iter_mut() {
                        if asset_server.get_path(*id).is_some() {
                            if tracker.path == asset_server.get_path(*id).unwrap().to_string() {
                                println!("HOLY MOLY IT DETECTS !!, now respawn {:?}", entity_name);
                                if children.is_some() {
                                    for child in children.unwrap().iter(){
                                        commands.entity(*child).despawn_recursive();
                                    }
                                }
                                commands.entity(entity)
                                    .remove::<BlueprintAssetsLoaded>()
                                    .remove::<SceneInstance>()
                                    .remove::<BlueprintAssetsLoadState>()
                                    .insert(SpawnHere);

                                break;
                            }
                        }
                    }
                }

            }
            _ => {}
        }
    }
}