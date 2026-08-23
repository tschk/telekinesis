import SwiftUI

struct ViewIrDocument: Decodable {
    let version: Int
    let root: [ViewIrNode]
}

struct ViewIrNode: Decodable, Identifiable {
    let id = UUID()
    let kind: String
    let content: String?
    let label: String?
    let axis: String?
    let children: [ViewIrNode]?

    enum CodingKeys: String, CodingKey {
        case kind, content, label, axis, children
    }
}

public struct ViewIrRootView: View {
    let document: ViewIrDocument

    public init(data: Data) throws {
        self.document = try JSONDecoder().decode(ViewIrDocument.self, from: data)
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(document.root) { node in
                    ViewIrNodeView(node: node)
                }
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Color.black)
    }
}

struct ViewIrNodeView: View {
    let node: ViewIrNode

    var body: some View {
        switch node.kind {
        case "text":
            Text(node.content ?? "")
                .foregroundStyle(Color.gray)
                .frame(maxWidth: .infinity, alignment: .leading)
        case "button":
            Text(node.label ?? node.content ?? "action")
                .foregroundStyle(Color.white)
        default:
            let children = node.children ?? []
            if node.axis == "row" || node.axis == "horizontal" {
                HStack(alignment: .center, spacing: 8) {
                    ForEach(children) { child in
                        ViewIrNodeView(node: child)
                    }
                }
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(children) { child in
                        ViewIrNodeView(node: child)
                    }
                }
            }
        }
    }
}

public struct PairRootView: View {
    public init() {}

    public var body: some View {
        if let url = Bundle.module.url(forResource: "fixture", withExtension: "json"),
           let data = try? Data(contentsOf: url),
           let view = try? ViewIrRootView(data: data)
        {
            view
        } else {
            Text("missing View IR fixture")
                .foregroundStyle(Color.gray)
        }
    }
}
