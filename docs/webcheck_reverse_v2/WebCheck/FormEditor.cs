using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormEditor : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	private string tabE;

	private string colE;

	[field: AccessedThroughProperty("OldText")]
	internal virtual TextBox OldText
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("NewText")]
	internal virtual TextBox NewText
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
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

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
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
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormEditor));
		this.OldText = new System.Windows.Forms.TextBox();
		this.NewText = new System.Windows.Forms.TextBox();
		this.OkB = new System.Windows.Forms.Button();
		this.NoB = new System.Windows.Forms.Button();
		this.Label2 = new System.Windows.Forms.Label();
		this.Label1 = new System.Windows.Forms.Label();
		base.SuspendLayout();
		this.OldText.Enabled = false;
		this.OldText.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OldText.Location = new System.Drawing.Point(33, 37);
		this.OldText.Name = "OldText";
		this.OldText.Size = new System.Drawing.Size(455, 30);
		this.OldText.TabIndex = 1;
		this.OldText.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.NewText.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NewText.Location = new System.Drawing.Point(33, 115);
		this.NewText.Name = "NewText";
		this.NewText.Size = new System.Drawing.Size(455, 30);
		this.NewText.TabIndex = 2;
		this.NewText.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.OkB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkB.Location = new System.Drawing.Point(356, 173);
		this.OkB.Name = "OkB";
		this.OkB.Size = new System.Drawing.Size(132, 40);
		this.OkB.TabIndex = 8;
		this.OkB.Text = "Зберегти";
		this.OkB.UseVisualStyleBackColor = true;
		this.NoB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoB.Location = new System.Drawing.Point(33, 173);
		this.NoB.Name = "NoB";
		this.NoB.Size = new System.Drawing.Size(132, 40);
		this.NoB.TabIndex = 7;
		this.NoB.Text = "Скасувати";
		this.NoB.UseVisualStyleBackColor = true;
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(28, 9);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(161, 25);
		this.Label2.TabIndex = 9;
		this.Label2.Text = "Старе значення";
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(28, 87);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(149, 25);
		this.Label1.TabIndex = 10;
		this.Label1.Text = "Нове значення";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(528, 241);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.Label2);
		base.Controls.Add(this.OkB);
		base.Controls.Add(this.NoB);
		base.Controls.Add(this.NewText);
		base.Controls.Add(this.OldText);
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormEditor";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Редактор Записей";
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormEditor(string textT, string oldT, string tabT, string colT)
	{
		base.Load += FormEditor_Load;
		InitializeComponent();
		Text = textT;
		OldText.Text = oldT;
		tabE = tabT;
		colE = colT;
	}

	private void FormEditor_Load(object sender, EventArgs e)
	{
		base.AcceptButton = OkB;
		base.CancelButton = NoB;
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		if (Operators.CompareString(NewText.Text.Trim(), "", TextCompare: false) == 0)
		{
			NewText.Focus();
			return;
		}
		Coding coding = new Coding();
		string text = All.l.TextToTextSQL(NewText.Text.Trim());
		if ((Operators.CompareString(tabE, "OPERATORS", TextCompare: false) == 0) & (Operators.CompareString(colE, "KEYPASS", TextCompare: false) == 0))
		{
			text = coding.Cod(text);
		}
		if (new UpdateInfa().UPDATE(tabE, colE, "1", text).errCode == 0)
		{
			if (Operators.CompareString(tabE, "TAXOBJECTS", TextCompare: false) == 0)
			{
				UpDateTaxObj(text);
			}
			Close();
		}
	}

	private void UpDateTaxObj(string ts)
	{
		switch (colE)
		{
		case "POINTADDR":
			All.A.PointAddr = All.l.TextSQLToText(ts);
			break;
		case "ORGNAME":
			All.A.OrgName = All.l.TextSQLToText(ts);
			break;
		case "POINTNAME":
			All.A.PointName = All.l.TextSQLToText(ts);
			break;
		case "INN":
			All.A.INN = ts;
			break;
		case "TIN":
			All.A.TIN = ts;
			break;
		case "FN":
			All.A.FN = ts;
			break;
		}
	}
}
