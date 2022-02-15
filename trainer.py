#!/usr/bin/python3
import torch
from torch import nn

import pytorch_lightning as pl

import struct, sys, json
from typing import Tuple

NUM_FEATURES = 2 * 6 * 64
LAYER_1 = 16

class Nnue(pl.LightningModule):
    def __init__(self):
        super(Nnue, self).__init__()

        self.features = nn.Linear(NUM_FEATURES, LAYER_1)
        self.layer1 = nn.Linear(LAYER_1, 1)

    def forward(self, features):
        l1_input = torch.clamp(self.features(features), 0.0, 1.0)
        return self.layer1(l1_input)

    def training_step(self, batch, batch_idx):
        features, target = batch
        value = torch.sigmoid(self(features) / 500)
        return (value - target)**2

    def configure_optimizers(self):
        return torch.optim.Adam(self.parameters())

class PositionSet(torch.utils.data.Dataset):
    def __init__(self, data: bytes):
        self.data = data

    def __len__(self) -> int:
        return len(self.data) // 66

    def __getitem__(self, index: int) -> Tuple[torch.sparse.Tensor, torch.Tensor]:
        content = struct.unpack("<" + "H" * 33, self.data[index*66:index*66+66])
        for i in range(33):
            if content[i] == 65535: break
        tensor = torch.sparse_coo_tensor([content[:i]], [1.0] * i, [NUM_FEATURES])
        outcome = content[32] / 2
        return tensor, torch.tensor([outcome])

if sys.argv[1] == "train":
    with open("data.bin", "rb") as f:
        dataset = PositionSet(f.read())
    dataloader = torch.utils.data.DataLoader(dataset)

    nnue = Nnue()
    trainer = pl.Trainer()
    trainer.fit(nnue, train_dataloaders=dataloader)
elif sys.argv[1] == "dump":
    nnue = Nnue.load_from_checkpoint(sys.argv[2])
    nnue.eval()

    def save_tensor(file, tensor, scale):
        file.write("[")
        for i in range(tensor.shape[0]):
            if len(tensor.shape) == 1:
                file.write(f"{round(tensor[i] * scale)},")
            else:
                save_tensor(file, tensor[i], scale)
                file.write(",")
        file.write("]")

    state = nnue.state_dict()

    with open("frozenight/model.rs", "w") as file:
        file.write("Nnue {")
        file.write("input_layer:")
        save_tensor(file, state["features.weight"].cpu().numpy().transpose(), 64)
        file.write(",input_layer_bias:")
        save_tensor(file, state["features.bias"].cpu().numpy(), 64)
        file.write(",hidden_layer:")
        save_tensor(file, state["layer1.weight"].cpu().numpy()[0], 1)
        file.write(",hidden_layer_bias:")
        file.write(f"{round(state['layer1.bias'].cpu().numpy()[0])},")
        file.write("}")