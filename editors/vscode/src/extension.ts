import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // The language server is launched as an external process that speaks LSP
  // over stdio: `sukti lsp`. The executable can be overridden by the user.
  const configuredPath = vscode.workspace
    .getConfiguration("sukti")
    .get<string>("path", "sukti");

  const serverOptions: ServerOptions = {
    command: configuredPath,
    args: ["lsp"],
    options: { shell: false },
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "sukti" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.{sukti,sk}"),
    },
  };

  client = new LanguageClient(
    "sukti",
    "Sūkti Language Server",
    serverOptions,
    clientOptions,
  );

  // Start the client, which also spawns the server process.
  await client.start();
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}
