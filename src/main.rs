mod convert;
mod crc32;
mod decode;
mod dom;
mod entities;
mod error;
mod formats;
mod http_srv;
mod inflate;
mod model;
mod osnet;
mod pathutil;
mod read_epub;
mod read_mobi;
mod read_office;
mod read_pdf;
mod read_text;
mod selftest;
mod write_office;
mod write_text;
mod zipio;

use error::Error;
use std::env;

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("serve");
    let result = match cmd {
        "serve" | "--serve" => {
            if args.first() == Some(&"serve".to_string()) {
                args.remove(0);
            }
            let port = take_flag(&mut args, "--port").and_then(|s| s.parse().ok()).unwrap_or(4173);
            let _host = take_flag(&mut args, "--host");
            http_srv::serve(port).map_err(Error::new)
        }
        "convert" => {
            args.remove(0);
            let to = take_flag(&mut args, "--to")
                .or_else(|| take_flag(&mut args, "-t"))
                .unwrap_or_else(|| "txt".into());
            let out_file = take_flag(&mut args, "-o").or_else(|| take_flag(&mut args, "--output"));
            let out_dir = take_flag(&mut args, "-d").or_else(|| take_flag(&mut args, "--out-dir"));
            http_srv::cli_convert(&args, &to, out_dir.as_deref(), out_file.as_deref())
        }
        "formats" => {
            println!("{}", formats::formats_json());
            Ok(())
        }
        "selftest" => selftest::run(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other if other.starts_with('-') => {
            // `convertetudo --port 4173` still serves
            let port = if other == "--port" {
                args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4173)
            } else {
                4173
            };
            http_srv::serve(port).map_err(Error::new)
        }
        other => {
            eprintln!("comando desconhecido: {other}\n");
            print_help();
            Err(Error::new("comando desconhecido"))
        }
    };
    if let Err(e) = result {
        eprintln!("erro: {}", e.msg);
        std::process::exit(1);
    }
}

fn take_flag(args: &mut Vec<String>, name: &str) -> Option<String> {
    if let Some(i) = args.iter().position(|a| a == name) {
        if i + 1 < args.len() {
            let v = args.remove(i + 1);
            args.remove(i);
            return Some(v);
        }
    }
    if let Some(i) = args.iter().position(|a| a.starts_with(&format!("{name}="))) {
        let v = args.remove(i);
        return v.split_once('=').map(|(_, x)| x.to_string());
    }
    None
}

fn print_help() {
    println!(
        "ConverteTudo — conversor local de documentos

Uso
  convertetudo                       abre o servidor em 0.0.0.0:4173
  convertetudo serve --port 4173
  convertetudo convert arquivo.epub -t txt -o saida.txt
  convertetudo convert *.epub -t pdf -d saida/
  convertetudo formats
  convertetudo selftest

O arquivo não sai da máquina. Nada é enviado a um servidor remoto.
Formatos de leitura: EPUB, HTML, DOCX, ODT, RTF, PDF, TXT, Markdown,
FB2, MOBI, CSV, XLSX, ODS, XPS, ABW, LaTeX.
Formatos de escrita: TXT, Markdown, HTML, RTF, DOCX, ODT, PDF, EPUB,
FB2, CSV, XLSX, LaTeX, JSON.
PDF escaneado (sem camada de texto) não entra: esta versão não faz OCR."
    );
}
