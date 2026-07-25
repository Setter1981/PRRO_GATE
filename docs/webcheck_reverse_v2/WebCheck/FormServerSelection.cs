using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormServerSelection : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("Servers")]
	private CheckedListBox _Servers;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	private bool NewBas;

	internal virtual CheckedListBox Servers
	{
		[CompilerGenerated]
		get
		{
			return _Servers;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Servers_SelectedIndexChanged;
			CheckedListBox servers = _Servers;
			if (servers != null)
			{
				servers.SelectedIndexChanged -= value2;
			}
			_Servers = value;
			servers = _Servers;
			if (servers != null)
			{
				servers.SelectedIndexChanged += value2;
			}
		}
	}

	internal virtual Button NoB
	{
		[CompilerGenerated]
		get
		{
			return _NoB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				noB.Click -= value2;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				noB.Click += value2;
			}
		}
	}

	internal virtual Button OkB
	{
		[CompilerGenerated]
		get
		{
			return _OkB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				okB.Click -= value2;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				okB.Click += value2;
			}
		}
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormServerSelection));
		this.Servers = new System.Windows.Forms.CheckedListBox();
		this.NoB = new System.Windows.Forms.Button();
		this.OkB = new System.Windows.Forms.Button();
		base.SuspendLayout();
		this.Servers.CheckOnClick = true;
		this.Servers.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Servers.FormattingEnabled = true;
		this.Servers.Location = new System.Drawing.Point(12, 12);
		this.Servers.Name = "Servers";
		this.Servers.Size = new System.Drawing.Size(422, 379);
		this.Servers.TabIndex = 0;
		this.NoB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoB.Location = new System.Drawing.Point(12, 406);
		this.NoB.Name = "NoB";
		this.NoB.Size = new System.Drawing.Size(132, 40);
		this.NoB.TabIndex = 5;
		this.NoB.Text = "Скасувати";
		this.NoB.UseVisualStyleBackColor = true;
		this.OkB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkB.Location = new System.Drawing.Point(303, 406);
		this.OkB.Name = "OkB";
		this.OkB.Size = new System.Drawing.Size(132, 40);
		this.OkB.TabIndex = 6;
		this.OkB.Text = "Вибрати";
		this.OkB.UseVisualStyleBackColor = true;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(447, 458);
		base.Controls.Add(this.OkB);
		base.Controls.Add(this.NoB);
		base.Controls.Add(this.Servers);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormServerSelection";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Налаштування АЦСК";
		base.ResumeLayout(false);
	}

	public FormServerSelection(bool NewBase)
	{
		base.Load += FormServerSelection_Load;
		NewBas = NewBase;
		InitializeComponent();
	}

	private void FormServerSelection_Load(object sender, EventArgs e)
	{
		base.AcceptButton = OkB;
		base.CancelButton = NoB;
		LoadServers();
	}

	private void LoadServers()
	{
		int count = All.SF.Servers(0).Count;
		for (int i = 0; i <= count; i = checked(i + 1))
		{
			Servers.Items.Add(All.SF.Servers(i).Name);
			_ = null;
		}
		if (NewBas)
		{
			Servers.SelectedIndex = All.A.AcskSettingsTemp;
		}
		else
		{
			Servers.SelectedIndex = All.A.AcskSettings;
		}
	}

	private void Servers_SelectedIndexChanged(object sender, EventArgs e)
	{
		checked
		{
			if (Servers.SelectedIndex >= 0)
			{
				int num = Servers.Items.Count - 1;
				for (int i = 0; i <= num; i++)
				{
					Servers.SetItemChecked(i, value: false);
				}
				Servers.SetItemChecked(Servers.SelectedIndex, value: true);
			}
		}
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		if (NewBas)
		{
			All.A.AcskSettingsTemp = Servers.SelectedIndex;
		}
		else if (All.A.Status && Servers.SelectedIndex != All.A.AcskSettings)
		{
			All.A.AcskSettings = Servers.SelectedIndex;
			All.A.AcskSettingsTemp = All.A.AcskSettings;
			All.f.StringWriteFN(All.A.FN, "Acsksettings", Conversions.ToString(All.A.AcskSettings));
		}
		Close();
	}
}
