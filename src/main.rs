use clap::Parser;
use env_logger::Env;
use log::{self, error, info};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, process::exit};

use gst::prelude::*;
use gstreamer::{
    self as gst, DebugGraphDetails, ElementFactory, GhostPad, PadDirection, PadProbeType,
};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// WHEP source url
    #[clap(short, long)]
    pub input_url: String,

    /// SRT output stream url
    #[clap(short, long, default_value_t = String::from("srt://0.0.0.0:1234?mode=listener"))]
    pub output_url: String,

    /// Output debug .dot files
    #[clap(long, default_value_t = false)]
    pub dot_debug: bool,
}

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = Args::parse();
    let whep_url = args.input_url;
    let output_url = args.output_url;
    let dot_debug = args.dot_debug;

    if dot_debug {
        let current_dir = format!(
            "{}",
            env::current_dir()
                .expect("could not get current directory")
                .display()
        );

        log::info!("Debugging .dot files to '{current_dir}'");
        unsafe {
            env::set_var("GST_DEBUG_DUMP_DOT_DIR", current_dir);
        }
    }

    gst::init().expect("Could not initiate GStreamer");

    info!("SRT output at {output_url}");
    info!("---");

    /*  NOTE:
       whepsrc was the first WHIP implementation in gstreamer, based on webrtcbin directly. it's present in gstwebrtchttp plugin.
       whepclientsrc has later been added and it is using the signaller interface on webrtcsrc and webrtcsink rust plugins. it's present in gstrswebrtc plugin.
       whepclientsrc is reusing a lot of functionallity and is supposed to deprecate whepsrc in the future.

       In this project we use the new whepclientsrc. Below is some dev code if you want to try out the old whepsrc implementation for some reason.
    */

    let use_whepsrc = false;
    let input = if use_whepsrc {
        //gstwebrtchttp::plugin_register_static().expect("Could not register gstwebrtchttp plugins");

        let audio_caps = "audio_caps=\"application/x-rtp, media=(string)audio, encoding-name=(string)opus, payload=(int)96, encoding-params=(string)2, clock-rate=(int)48000\"";
        format!(
            "whepsrc name=input use-link-headers=false whep-endpoint=\"{whep_url}\" {audio_caps} video-caps=\"\""
        )
    } else {
        gstrswebrtc::plugin_register_static().expect("Could not register gstrswebrtc plugins");

        format!("whepclientsrc name=input signaller::whep-endpoint=\"{whep_url}\"")
    };

    let mixer = "liveadder name=mixer"; //this could be audiomixer also, but liveadder will do fine here

    let pipeline_str = format!(
        "{input} audiotestsrc wave=silence is-live=true ! audio/x-raw,format=F32LE,rate=48000,channels=2 ! {mixer} ! avenc_aac ! aacparse ! mux. \
        mpegtsmux name=mux alignment=7 ! queue ! srtsink uri=\"{output_url}\" sync=false wait-for-connection=false latency=100"
    );

    let mut context = gst::ParseContext::new();
    let pipeline = match gst::parse::launch_full(
        &pipeline_str,
        Some(&mut context),
        gst::ParseFlags::empty(),
    ) {
        Ok(pipeline) => pipeline,
        Err(err) => {
            if let Some(gst::ParseError::NoSuchElement) = err.kind::<gst::ParseError>() {
                error!("Missing element(s): {:?}", context.missing_elements());
            } else {
                error!("Failed to parse pipeline: {err}");
            }

            std::process::exit(-1)
        }
    };

    let pipeline = pipeline
        .dynamic_cast::<gst::Pipeline>()
        .expect("could not cast pipeline");
    let pipeline_clone = pipeline.clone();

    let mixer = pipeline
        .by_name("mixer")
        .expect("could not find mixer element");
    let mixer_clone = mixer.clone();

    let input_whep_bin = pipeline
        .by_name("input")
        .expect("could not get whep input bin");

    let _ = ctrlc::set_handler(move || {
        info!("exit.. shutting down");

        pipeline_clone
            .set_state(gst::State::Null)
            .expect("Unable to set the pipeline to the `Null` state");

        std::thread::sleep(std::time::Duration::from_secs(1));

        exit(0);
    });

    let bus = pipeline.bus().unwrap();

    let pipeline_clone = pipeline.clone();

    pipeline.connect_deep_element_added(move |pipe, bin, elem| {
        let elem_type = elem.type_().to_string();
        let _ = pipe;
        let _ = bin;

        if elem_type == "GstWebRTCBin" {
            
            elem.connect_pad_added(move |elem, pad| {
                info!("webrtcbin pad added: '{}'", pad.name());

                /*
                   Note: When receiving multiple audio tracks (ssrcs), the first track is automatically exposed 'out' of the whepclientsrc bin
                   Other tracks are _not_ automatically exposed, so we have to handle that manually. That is why we listen for pad_added on webrtcbin and
                   then ghostpad our way out of the bins.
                */

                let caps = pad
                    .current_caps()
                    .unwrap_or_else(|| panic!("could not get current_caps on pad {}", pad.name()));
                let s = caps
                    .structure(0)
                    .expect("could not get structure 0 on caps");

                //info!("full structure: {:#?}", s);

                let media_type = s
                    .get::<String>("media")
                    .expect("could not get media from caps structure");

                if !pad.is_linked() {
                    //this is not automatically linked, we have to handle it. 
                    info!("pad '{}' is not automatically linked, handling ghostpads. media_type: {media_type}", pad.name());

                    let parent = elem.parent().expect("could not get webrtcbin parent");
                    let parent = parent
                        .dynamic_cast_ref::<gst::Bin>()
                        .expect("could not cast webrtcbin parent");

                    let new_pad_name = format!("{}_{}", media_type, pad.name());

                    let ghostpad = GhostPad::builder(PadDirection::Src)
                        .with_target(pad)
                        .expect("could not create ghostpad")
                        .name(&new_pad_name)
                        .build();
                    parent
                        .add_pad(&ghostpad)
                        .expect("could not add ghostpad to parent");

                    let parent_parent = parent.parent().expect("could not get parent parent");
                    if let Some(_pipe) = parent_parent.dynamic_cast_ref::<gst::Pipeline>() {
                        //info!("found pipeline.. no more ghostpads needed");
                    } else {
                        let parent_parent = parent_parent
                            .dynamic_cast_ref::<gst::Bin>()
                            .expect("could cast webrtcbin parent parent");

                        let ghostpad2 = GhostPad::builder(PadDirection::Src)
                            .with_target(&ghostpad)
                            .expect("could not create ghostpad2 with target ghostpad")
                            .name(&new_pad_name)
                            .build();
                        parent_parent
                            .add_pad(&ghostpad2)
                            .expect("could not add ghostpad2");
                    }
                }
            });
        }
    });

    input_whep_bin.connect_pad_added(move |elem, pad| {
        info!(
            "pad added on {} named '{}': '{}'",
            elem.type_(),
            elem.name(),
            pad.name()
        );

        let pipeline_clone = pipeline_clone.clone();
        let mixer_clone = mixer_clone.clone();

        pad.add_probe(PadProbeType::BUFFER, move |pad, _probe_info| {
            let caps = pad.current_caps().unwrap();
            let media_type = caps.structure(0).unwrap().get::<String>("media").unwrap();

            info!("getting {media_type} track");
            match media_type.as_str() {
                "audio" => {
                    let pipe_bin = pipeline_clone
                        .dynamic_cast_ref::<gst::Bin>()
                        .expect("could not cast pipeline to bin");

                    let decodebin = ElementFactory::make("decodebin")
                        .build()
                        .expect("could not create decodebin");
                    pipe_bin
                        .add(&decodebin)
                        .expect("could not add decodebin to pipe_bin");
                    decodebin
                        .sync_state_with_parent()
                        .expect("could not sync_state on decode_bin");

                    let pipe_bin_clone = pipe_bin.clone();

                    let mixer_clone = mixer_clone.clone();
                    decodebin.connect_pad_added(move |elem, pad| {
                        info!("pad '{}' added on decodebin '{}'", pad.name(), elem.name());

                        let audioconvert = ElementFactory::make("audioconvert")
                            .build()
                            .expect("could not create audioconvert");
                        let audioresample = ElementFactory::make("audioresample")
                            .build()
                            .expect("could not create audioresample");
                        let caps = ElementFactory::make("capsfilter")
                            .build()
                            .expect("could not create capsfiler");
                        caps.set_property_from_str("caps", "audio/x-raw,format=F32LE,rate=48000");

                        let elements = [&audioconvert, &audioresample, &caps];

                        pipe_bin_clone
                            .add_many(elements)
                            .expect("could not add_many");
                        for elem in elements {
                            elem.sync_state_with_parent()
                                .expect("could not sync_state_with_parent");
                        }

                        gst::Element::link_many(elements).expect("could not link many on elements");

                        //-- setup links from decodebin leg to audiomixer --
                        let caps_src_pad = caps.static_pad("src").unwrap();

                        let mixer_input_pad = mixer_clone
                            .request_pad_simple("sink_%u")
                            .expect("could not get audio mixer input pad");

                        caps_src_pad
                            .link(&mixer_input_pad)
                            .expect("could not link input audio to audiomixer");

                        //link decodebin pad to audioconvert
                        pad.link(&audioconvert.static_pad("sink").unwrap())
                            .expect("could not link decodebin to audioconvert sink");
                    });

                    //link from webrtcbin to decodebin
                    let decodebin_pad = decodebin.iterate_sink_pads().next().unwrap().unwrap();

                    pad.link(&decodebin_pad)
                        .expect("could not link from webrtcbin audio pad to decodebin");
                }
                "video" => {
                    //TODO: this should be sent to muxer maybe?

                    let fakesink = ElementFactory::make("fakesink")
                        .build()
                        .expect("could not create video fakesink");

                    pipeline_clone
                        .add(&fakesink)
                        .expect("could not add video fakesink to pipeline");
                    fakesink
                        .sync_state_with_parent()
                        .expect("could not sync state on fakesink");
                    let fakesink_pad = fakesink
                        .static_pad("sink")
                        .expect("could not get fakesink pad");
                    pad.link(&fakesink_pad)
                        .expect("could not link video sinkpad sink");
                }
                _ => {
                    error!("unhandled media type");
                }
            }

            gstreamer::PadProbeReturn::Remove
        });
    });

    // Start pipeline - ICE role is configured via webrtcbin-ready signal
    pipeline
        .set_state(gst::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state");

    let pipeline_clone = pipeline.clone();

    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;

        match msg.view() {
            MessageView::StateChanged(state) => {
                if !state
                    .src()
                    .unwrap()
                    .type_()
                    .to_string()
                    .contains("GstPipeline")
                {
                    continue;
                }

                log::debug!(
                    "pipeline change: {:?} -> {:?}",
                    state.old(),
                    state.current()
                );

                if dot_debug {
                    let pipe_bin = pipeline_clone.dynamic_cast_ref::<gst::Bin>().unwrap();
                    debug_pipeline(pipe_bin, &format!("{:?}", state.current()));
                }
            }
            MessageView::Eos(..) => break,
            MessageView::Error(err) => {
                error!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );

                if dot_debug {
                    let pipe_bin = pipeline_clone.dynamic_cast_ref::<gst::Bin>().unwrap();
                    debug_pipeline(pipe_bin, "error");
                }

                break;
            }
            _ => (),
        }
    }

    pipeline
        .set_state(gst::State::Null)
        .expect("Unable to set the pipeline to the `Null` state");

    std::thread::sleep(std::time::Duration::from_secs(1));
}

fn debug_pipeline(pipe: &gst::Bin, str: &str) {
    let epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    let filename = format!("{:?}-{}", epoch.as_secs(), str);
    pipe.debug_to_dot_file(DebugGraphDetails::ALL, &filename);

    info!(
        "debugging to file: '{filename}.dot'. Use xdot application to view or convert to svg with 'dot -Tsvg {filename}.dot -o {filename}.svg'"
    );

    /*
    use std::{os::unix::process::CommandExt, process::Command};
    let _ = std::process::Command::new("dot")
        .arg("-Tsvg")
        .arg(format!("{filename}.dot"))
        .arg("-o")
        .arg(format!("{filename}.svg"))
        .spawn();
    */
}
