//! Registry plugin take a VFile attribute return from a node and  add the result of an registry function to the attribute of this node

use std::io::BufReader;
use std::fmt::Debug;

use tap::plugin;
use tap::node::Node;
use tap::vfile::VFile;
use tap::value::Value;
use tap::config_schema;
use tap::error::RustructError;
use tap::attribute::Attributes;
use tap::tree::{Tree, TreeNodeId, TreeNodeIdSchema};
use tap::plugin::{PluginInfo, PluginInstance, PluginConfig, PluginArgument, PluginResult, PluginEnvironment};

use schemars::{JsonSchema};
use rwinreg::{hive, vk, nk};
use serde::{Serialize, Deserialize};

plugin!("registry", "Windows", "Parse registry file", RegistryPlugin, Arguments);

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Arguments
{
  #[schemars(with = "TreeNodeIdSchema")] 
  file : TreeNodeId,
}

#[derive(Debug, Serialize, Deserialize,Default)]
pub struct Results
{
}

#[derive(Default)]
pub struct RegistryPlugin
{
}

impl RegistryPlugin
{
  fn run(&mut self, args : Arguments, env : PluginEnvironment) -> anyhow::Result<Results>
  {
    let file_node = env.tree.get_node_from_id(args.file).ok_or(RustructError::ArgumentNotFound("file"))?;
    file_node.value().add_attribute(self.name(), None, None); 
    let data = file_node.value().get_value("data").ok_or(RustructError::ValueNotFound("data"))?;
    let data_builder = data.try_as_vfile_builder().ok_or(RustructError::ValueTypeMismatch)?;
    let file = data_builder.open()?;

    let file = BufReader::new(file); 
    
    let mut hive = match hive::Hive::from_source(file)
    {
      Ok(hive) => hive,
      Err(err) => return Err(RustructError::Unknown(err.to_string()).into()),
    };

    let mut reg_base = match hive.get_root_node()
    {
      Ok(reg_base) => reg_base,
      Err(err) => return Err(RustructError::Unknown(err.to_string()).into()),
    };

    let file = data_builder.open()?;
    let mut file = BufReader::new(file);

    iterate_key(&mut reg_base, &env.tree, &mut file, args.file);

    Ok(Results{})
  }
}
                                   
pub fn iterate_key(key : &mut nk::NodeKey, tree : &Tree, mut file : &mut dyn VFile, parent_id : TreeNodeId)
{
  let node = Node::new(key.key_name().to_string());

  let mut attributes = Attributes::new();

  if let Some(last_written) = key.get_last_written()
  {
    attributes.add_attribute("last_written", *last_written, None);
    node.value().add_attribute("registry", attributes, None);
  }
  let parent_id = tree.add_child(parent_id, node).unwrap();

  loop
  {
    match key.get_next_value(&mut file)
    {
      Ok(next_value) =>
      {
        match next_value
        {
          Some(mut value) => 
          {
            if value.get_size() > 100*1024*1024
            {
              continue; //can add attribute but without data so we have key name
            }
            let _ = value.read_value(&mut file); //check return value
            let data = match value.decode_data() 
            {
              Ok(data) => match data
              {
                Some(data) => match data
                {
                  vk::Data::None => Value::from(None),
                  vk::Data::String(string) => Value::String(string),
                  vk::Data::Int32(num) => Value::I32(num),
                }
                None => Value::from(None),
              }
              Err(_err) => Value::from(None),
            };
            let name = value.get_name().to_string();
            let name = match name.len()
            {
              0 => "default".to_string(),
              _ => name,
            };
            let subnode = Node::new(name);
            let mut registry_attribute = Attributes::new();
            //attributes.add_attribute("type", typename, None);//XXX add type?
            registry_attribute.add_attribute("data", data, None);
            subnode.value().add_attribute("registry", registry_attribute, None);
            tree.add_child(parent_id, subnode).unwrap();
          },
          None => break,
        }
      }
      Err(err) => {println!("error {}", err); return },
    }
  }

  loop 
  {
    match key.get_next_key(&mut file)
    {
      Ok(next_key) =>
      {
        match next_key {
          Some(mut next_key) => iterate_key(&mut next_key, tree, &mut file, parent_id),
          None => break,
        }
      },
      Err(err) => {println!("error {}", err); return} ,
    }
  }
}
