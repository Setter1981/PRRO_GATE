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
			EventHandler eventHandler = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click -= eventHandler;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click += eventHandler;
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
			EventHandler eventHandler = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click -= eventHandler;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click += eventHandler;
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
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_007b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0085: Expected O, but got Unknown
		//IL_00ef: Unknown result type (might be due to invalid IL or missing references)
		//IL_00f9: Expected O, but got Unknown
		//IL_0163: Unknown result type (might be due to invalid IL or missing references)
		//IL_016d: Expected O, but got Unknown
		//IL_01ed: Unknown result type (might be due to invalid IL or missing references)
		//IL_01f7: Expected O, but got Unknown
		//IL_0280: Unknown result type (might be due to invalid IL or missing references)
		//IL_028a: Expected O, but got Unknown
		//IL_0305: Unknown result type (might be due to invalid IL or missing references)
		//IL_030f: Expected O, but got Unknown
		//IL_040a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0414: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormEditor));
		OldText = new TextBox();
		NewText = new TextBox();
		OkB = new Button();
		NoB = new Button();
		Label2 = new Label();
		Label1 = new Label();
		((Control)this).SuspendLayout();
		((Control)OldText).Enabled = false;
		((Control)OldText).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OldText).Location = new Point(33, 37);
		((Control)OldText).Name = "OldText";
		((Control)OldText).Size = new Size(455, 30);
		((Control)OldText).TabIndex = 1;
		OldText.TextAlign = (HorizontalAlignment)2;
		((Control)NewText).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NewText).Location = new Point(33, 115);
		((Control)NewText).Name = "NewText";
		((Control)NewText).Size = new Size(455, 30);
		((Control)NewText).TabIndex = 2;
		NewText.TextAlign = (HorizontalAlignment)2;
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(356, 173);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(132, 40);
		((Control)OkB).TabIndex = 8;
		((ButtonBase)OkB).Text = "Зберегти";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(33, 173);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(132, 40);
		((Control)NoB).TabIndex = 7;
		((ButtonBase)NoB).Text = "Скасувати";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(28, 9);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(161, 25);
		((Control)Label2).TabIndex = 9;
		Label2.Text = "Старе значення";
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(28, 87);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(149, 25);
		((Control)Label1).TabIndex = 10;
		Label1.Text = "Нове значення";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(528, 241);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)Label2);
		((Control)this).Controls.Add((Control)(object)OkB);
		((Control)this).Controls.Add((Control)(object)NoB);
		((Control)this).Controls.Add((Control)(object)NewText);
		((Control)this).Controls.Add((Control)(object)OldText);
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormEditor";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Редактор Записей";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormEditor(string textT, string oldT, string tabT, string colT)
	{
		((Form)this).Load += FormEditor_Load;
		InitializeComponent();
		((Form)this).Text = textT;
		OldText.Text = oldT;
		tabE = tabT;
		colE = colT;
	}

	private void FormEditor_Load(object sender, EventArgs e)
	{
		((Form)this).AcceptButton = (IButtonControl)(object)OkB;
		((Form)this).CancelButton = (IButtonControl)(object)NoB;
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		if (Operators.CompareString(NewText.Text.Trim(), "", false) == 0)
		{
			((Control)NewText).Focus();
			return;
		}
		Coding coding = new Coding();
		string text = All.l.TextToTextSQL(NewText.Text.Trim());
		if ((Operators.CompareString(tabE, "OPERATORS", false) == 0) & (Operators.CompareString(colE, "KEYPASS", false) == 0))
		{
			text = coding.Cod(text);
		}
		if (new UpdateInfa().UPDATE(tabE, colE, "1", text).errCode == 0)
		{
			if (Operators.CompareString(tabE, "TAXOBJECTS", false) == 0)
			{
				UpDateTaxObj(text);
			}
			((Form)this).Close();
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
