import SwiftUI

struct BenchmarkView: View {
    @StateObject private var model = BenchmarkViewModel()

    var body: some View {
        NavigationView {
            VStack(alignment: .leading, spacing: 16) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 12) {
                        Button {
                            model.run(.realData)
                        } label: {
                            Label("Real Data", systemImage: "film.stack")
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(model.isRunning)

                        Button {
                            model.run(.smallSmoke)
                        } label: {
                            Label("Smoke", systemImage: "bolt")
                        }
                        .buttonStyle(.bordered)
                        .disabled(model.isRunning)

                        Button {
                            model.run(.deviceValidation)
                        } label: {
                            Label("Device", systemImage: "iphone")
                        }
                        .buttonStyle(.bordered)
                        .disabled(model.isRunning)

                        Menu {
                            ForEach(model.memoryPresets) { preset in
                                Button(preset.title) {
                                    model.run(.memory(preset))
                                }
                            }
                        } label: {
                            Label("Memory", systemImage: "memorychip")
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(model.isRunning || model.memoryScenarioRequiresRelaunch)

                        Button {
                            model.run(.fullDefault)
                        } label: {
                            Label("Default", systemImage: "speedometer")
                        }
                        .buttonStyle(.bordered)
                        .disabled(model.isRunning)

                        Button {
                            model.run(.compactDefault)
                        } label: {
                            Label("Compact", systemImage: "archivebox")
                        }
                        .buttonStyle(.bordered)
                        .disabled(model.isRunning)
                    }
                }

                HStack(spacing: 10) {
                    if model.isRunning {
                        ProgressView()
                    }

                    Text(model.status)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }

                Text(model.summary)
                    .font(.system(.caption, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(12)
                    .background(.quaternary.opacity(0.25))
                    .clipShape(RoundedRectangle(cornerRadius: 8))

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
        .task {
            await model.runLaunchScenarioIfPresent()
        }
    }
}

#Preview {
    BenchmarkView()
}
