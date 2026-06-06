import SwiftUI

struct BenchmarkView: View {
    @StateObject private var model = BenchmarkViewModel()

    var body: some View {
        NavigationView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 12) {
                    Button {
                        model.run(.smallSmoke)
                    } label: {
                        Label("Smoke", systemImage: "bolt")
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(model.isRunning)

                    Button {
                        model.run(.fullDefault)
                    } label: {
                        Label("Default", systemImage: "speedometer")
                    }
                    .buttonStyle(.bordered)
                    .disabled(model.isRunning)
                }

                HStack(spacing: 10) {
                    if model.isRunning {
                        ProgressView()
                    }

                    Text(model.status)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }

                ScrollView {
                    Text(model.output)
                        .font(.system(.footnote, design: .monospaced))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                }
                .background(.quaternary.opacity(0.25))
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }
            .padding()
            .navigationTitle("VectorKit Bench")
        }
        .navigationViewStyle(.stack)
    }
}

#Preview {
    BenchmarkView()
}
