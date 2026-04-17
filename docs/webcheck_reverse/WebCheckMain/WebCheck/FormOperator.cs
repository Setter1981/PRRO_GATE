using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormOperator : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("KeyB")]
	private Button _KeyB;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	[CompilerGenerated]
	[AccessedThroughProperty("CBR")]
	private CheckBox _CBR;

	private readonly bool AddOperator;

	private int indexOp;

	private string tFioOp;

	private string tInnOp;

	private string tPasOp;

	private string tKeyOp;

	internal virtual Button KeyB
	{
		[CompilerGenerated]
		get
		{
			return _KeyB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = KeyB_Click;
			Button keyB = _KeyB;
			if (keyB != null)
			{
				((Control)keyB).Click -= eventHandler;
			}
			_KeyB = value;
			keyB = _KeyB;
			if (keyB != null)
			{
				((Control)keyB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label11")]
	internal virtual Label Label11
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label10")]
	internal virtual Label Label10
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label9")]
	internal virtual Label Label9
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PasOpT")]
	internal virtual TextBox PasOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("KeyOpT")]
	internal virtual TextBox KeyOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("InnOpT")]
	internal virtual TextBox InnOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("FioOpT")]
	internal virtual TextBox FioOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label8")]
	internal virtual Label Label8
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
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

	internal virtual CheckBox CBR
	{
		[CompilerGenerated]
		get
		{
			return _CBR;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = CBR_CheckedChanged;
			CheckBox cBR = _CBR;
			if (cBR != null)
			{
				cBR.CheckedChanged -= eventHandler;
			}
			_CBR = value;
			cBR = _CBR;
			if (cBR != null)
			{
				cBR.CheckedChanged += eventHandler;
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
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_007f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0089: Expected O, but got Unknown
		//IL_008a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0094: Expected O, but got Unknown
		//IL_012d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0137: Expected O, but got Unknown
		//IL_01b5: Unknown result type (might be due to invalid IL or missing references)
		//IL_01bf: Expected O, but got Unknown
		//IL_0237: Unknown result type (might be due to invalid IL or missing references)
		//IL_0241: Expected O, but got Unknown
		//IL_02b0: Unknown result type (might be due to invalid IL or missing references)
		//IL_02ba: Expected O, but got Unknown
		//IL_0337: Unknown result type (might be due to invalid IL or missing references)
		//IL_0341: Expected O, but got Unknown
		//IL_03af: Unknown result type (might be due to invalid IL or missing references)
		//IL_03b9: Expected O, but got Unknown
		//IL_0427: Unknown result type (might be due to invalid IL or missing references)
		//IL_0431: Expected O, but got Unknown
		//IL_04ab: Unknown result type (might be due to invalid IL or missing references)
		//IL_04b5: Expected O, but got Unknown
		//IL_0524: Unknown result type (might be due to invalid IL or missing references)
		//IL_052e: Expected O, but got Unknown
		//IL_05ac: Unknown result type (might be due to invalid IL or missing references)
		//IL_05b6: Expected O, but got Unknown
		//IL_0643: Unknown result type (might be due to invalid IL or missing references)
		//IL_064d: Expected O, but got Unknown
		//IL_07c7: Unknown result type (might be due to invalid IL or missing references)
		//IL_07d1: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormOperator));
		KeyB = new Button();
		Label11 = new Label();
		Label10 = new Label();
		Label9 = new Label();
		PasOpT = new TextBox();
		KeyOpT = new TextBox();
		InnOpT = new TextBox();
		FioOpT = new TextBox();
		Label8 = new Label();
		NoB = new Button();
		OkB = new Button();
		CBR = new CheckBox();
		((Control)this).SuspendLayout();
		((Control)KeyB).Location = new Point(222, 114);
		((Control)KeyB).Name = "KeyB";
		((Control)KeyB).Size = new Size(53, 30);
		((Control)KeyB).TabIndex = 25;
		((Control)KeyB).TabStop = false;
		((ButtonBase)KeyB).Text = "...";
		((ButtonBase)KeyB).UseVisualStyleBackColor = true;
		Label11.AutoSize = true;
		((Control)Label11).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label11).Location = new Point(32, 157);
		((Control)Label11).Name = "Label11";
		((Control)Label11).Size = new Size(202, 25);
		((Control)Label11).TabIndex = 33;
		Label11.Text = "Пароль ключа ЕЦП *";
		Label10.AutoSize = true;
		((Control)Label10).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label10).Location = new Point(32, 117);
		((Control)Label10).Name = "Label10";
		((Control)Label10).Size = new Size(121, 25);
		((Control)Label10).TabIndex = 32;
		Label10.Text = "Ключ ЕЦП *";
		Label9.AutoSize = true;
		((Control)Label9).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label9).Location = new Point(32, 77);
		((Control)Label9).Name = "Label9";
		((Control)Label9).Size = new Size(159, 25);
		((Control)Label9).TabIndex = 27;
		Label9.Text = "ІНН оператора *";
		((Control)PasOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PasOpT).Location = new Point(293, 154);
		((Control)PasOpT).Name = "PasOpT";
		((Control)PasOpT).Size = new Size(430, 30);
		((Control)PasOpT).TabIndex = 31;
		PasOpT.TextAlign = (HorizontalAlignment)2;
		((Control)KeyOpT).Enabled = false;
		((Control)KeyOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)KeyOpT).Location = new Point(293, 114);
		((Control)KeyOpT).Name = "KeyOpT";
		((Control)KeyOpT).Size = new Size(430, 30);
		((Control)KeyOpT).TabIndex = 30;
		KeyOpT.TextAlign = (HorizontalAlignment)2;
		((Control)InnOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)InnOpT).Location = new Point(293, 74);
		((Control)InnOpT).Name = "InnOpT";
		((Control)InnOpT).Size = new Size(430, 30);
		((Control)InnOpT).TabIndex = 29;
		InnOpT.TextAlign = (HorizontalAlignment)2;
		((Control)FioOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)FioOpT).Location = new Point(293, 33);
		((Control)FioOpT).Name = "FioOpT";
		((Control)FioOpT).Size = new Size(430, 30);
		((Control)FioOpT).TabIndex = 28;
		FioOpT.TextAlign = (HorizontalAlignment)2;
		Label8.AutoSize = true;
		((Control)Label8).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label8).Location = new Point(32, 36);
		((Control)Label8).Name = "Label8";
		((Control)Label8).Size = new Size(159, 25);
		((Control)Label8).TabIndex = 26;
		Label8.Text = "ПІБ оператора *";
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(37, 218);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(132, 40);
		((Control)NoB).TabIndex = 38;
		((ButtonBase)NoB).Text = "Скасувати";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(591, 218);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(132, 40);
		((Control)OkB).TabIndex = 37;
		((ButtonBase)OkB).Text = "Ок";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		((ButtonBase)CBR).AutoSize = true;
		((Control)CBR).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CBR).Location = new Point(293, 217);
		((Control)CBR).Name = "CBR";
		((Control)CBR).Size = new Size(260, 44);
		((Control)CBR).TabIndex = 39;
		((ButtonBase)CBR).Text = "Вказати відкладений ключ\r\nдля цього оператора";
		((ButtonBase)CBR).UseVisualStyleBackColor = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(756, 279);
		((Control)this).Controls.Add((Control)(object)CBR);
		((Control)this).Controls.Add((Control)(object)NoB);
		((Control)this).Controls.Add((Control)(object)OkB);
		((Control)this).Controls.Add((Control)(object)KeyB);
		((Control)this).Controls.Add((Control)(object)Label11);
		((Control)this).Controls.Add((Control)(object)Label10);
		((Control)this).Controls.Add((Control)(object)Label9);
		((Control)this).Controls.Add((Control)(object)PasOpT);
		((Control)this).Controls.Add((Control)(object)KeyOpT);
		((Control)this).Controls.Add((Control)(object)InnOpT);
		((Control)this).Controls.Add((Control)(object)FioOpT);
		((Control)this).Controls.Add((Control)(object)Label8);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormOperator";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "FormOperator";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormOperator(bool NewOperator, string IND = "0", string FIO = "", string INN = "", string PASS = "", string KEY = "")
	{
		((Form)this).Load += FormOperator_Load;
		InitializeComponent();
		Coding coding = new Coding();
		AddOperator = NewOperator;
		if (!Versioned.IsNumeric((object)IND))
		{
			IND = "0";
		}
		indexOp = Conversions.ToInteger(IND);
		tFioOp = FIO;
		tInnOp = INN;
		tPasOp = coding.DeCod(PASS);
		tKeyOp = KEY;
	}

	private void FormOperator_Load(object sender, EventArgs e)
	{
		((Form)this).CancelButton = (IButtonControl)(object)NoB;
		((Form)this).AcceptButton = (IButtonControl)(object)OkB;
		if (AddOperator)
		{
			((Form)this).Text = "Додати нового оператора";
			((Control)FioOpT).Enabled = true;
			((Control)InnOpT).Enabled = true;
			((Control)PasOpT).Enabled = true;
			((Control)KeyOpT).Enabled = false;
			((Control)CBR).Enabled = false;
		}
		else
		{
			((Form)this).Text = "Редагувати оператора";
			((Control)FioOpT).Enabled = true;
			((Control)InnOpT).Enabled = false;
			((Control)PasOpT).Enabled = true;
			((Control)KeyOpT).Enabled = false;
			FioOpT.Text = tFioOp;
			InnOpT.Text = tInnOp;
			PasOpT.Text = "*********";
			KeyOpT.Text = tKeyOp;
			if (Operators.CompareString(tInnOp[0].ToString(), "R", false) == 0)
			{
				((Control)CBR).Enabled = false;
			}
			else
			{
				((Control)CBR).Enabled = true;
			}
		}
		((Control)this).Show();
		((Control)FioOpT).Focus();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		//IL_019b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0100: Unknown result type (might be due to invalid IL or missing references)
		if (AddOperator)
		{
			if (Operators.CompareString(FioOpT.Text.Trim(), "", false) == 0)
			{
				((Control)FioOpT).Focus();
			}
			else if (Operators.CompareString(InnOpT.Text.Trim(), "", false) == 0)
			{
				((Control)InnOpT).Focus();
			}
			else if (Operators.CompareString(KeyOpT.Text.Trim(), "", false) == 0)
			{
				string text = PathKey();
				if (Operators.CompareString(text, "", false) != 0)
				{
					KeyOpT.Text = text;
					((Control)PasOpT).Focus();
				}
			}
			else if (Operators.CompareString(PasOpT.Text.Trim(), "", false) == 0)
			{
				((Control)PasOpT).Focus();
			}
			else if (new OperatorsAll().CountOperators(InnOpT.Text.Trim()) > 0)
			{
				Interaction.MsgBox((object)"Оператор з таким ІНН вже є!", (MsgBoxStyle)48, (object)"Новий оператор");
			}
			else if (All.l.AddNewOperator(All.l.TextToTextXML(FioOpT.Text), KeyOpT.Text, PasOpT.Text, InnOpT.Text))
			{
				Application.DoEvents();
				((Form)this).Close();
			}
		}
		else if (CBR.Checked)
		{
			if (new OperatorsAll().CountOperators("R" + InnOpT.Text.Trim()) > 0)
			{
				Interaction.MsgBox((object)"Відкладений ключ вже є!", (MsgBoxStyle)48, (object)"Новий оператор");
			}
			else if (Operators.CompareString(KeyOpT.Text.Trim(), "", false) == 0)
			{
				string text2 = PathKey();
				if (Operators.CompareString(text2, "", false) != 0)
				{
					KeyOpT.Text = text2;
					((Control)PasOpT).Focus();
				}
			}
			else if (Operators.CompareString(PasOpT.Text.Trim(), "", false) == 0)
			{
				((Control)PasOpT).Focus();
			}
			else if (All.l.AddNewOperator(All.l.TextToTextXML("Відкладений ключ"), KeyOpT.Text, PasOpT.Text, "R" + InnOpT.Text))
			{
				Application.DoEvents();
				((Form)this).Close();
			}
		}
		else
		{
			UpdateInfa updateInfa = new UpdateInfa();
			if (Operators.CompareString(FioOpT.Text.Trim(), tFioOp, false) != 0)
			{
				string newInfa = FioOpT.Text.Trim();
				updateInfa.UPDATE("OPERATORS", "OPERATORNAME", Conversions.ToString(indexOp), newInfa);
			}
			if (Operators.CompareString(PasOpT.Text.Trim(), "*********", false) != 0 && Operators.CompareString(PasOpT.Text.Trim(), tPasOp, false) != 0)
			{
				string newInfa = PasOpT.Text.Trim();
				newInfa = new Coding().Cod(newInfa);
				updateInfa.UPDATE("OPERATORS", "KEYPASS", Conversions.ToString(indexOp), newInfa);
			}
			if (Operators.CompareString(KeyOpT.Text.Trim(), tKeyOp, false) != 0)
			{
				string newInfa = KeyOpT.Text.Trim();
				updateInfa.UPDATE("OPERATORS", "KEYPATH", Conversions.ToString(indexOp), newInfa);
			}
			IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\dat.ini");
			iniHGB.WriteString(tInnOp, "EndKey", "");
			iniHGB.WriteString(tInnOp, "Updated", "0");
			((Form)this).Close();
		}
	}

	private void KeyB_Click(object sender, EventArgs e)
	{
		string text = PathKey();
		if (Operators.CompareString(text, "", false) != 0)
		{
			KeyOpT.Text = text;
			((Control)PasOpT).Focus();
		}
	}

	private string PathKey()
	{
		//IL_0000: Unknown result type (might be due to invalid IL or missing references)
		//IL_0006: Expected O, but got Unknown
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		//IL_0018: Invalid comparison between Unknown and I4
		OpenFileDialog val = new OpenFileDialog();
		((FileDialog)val).Filter = "Key Files|*.dat;*.pfx;*.zs2;*.pk8;*.jks|All Files|*.*";
		if ((int)((CommonDialog)val).ShowDialog() == 1)
		{
			return ((FileDialog)val).FileName;
		}
		return "";
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void CBR_CheckedChanged(object sender, EventArgs e)
	{
		if (CBR.Checked)
		{
			((Form)this).Text = "Відкладений ключ для оператора";
			((Control)FioOpT).Enabled = false;
			((Control)InnOpT).Enabled = false;
			((Control)PasOpT).Enabled = true;
			((Control)KeyOpT).Enabled = false;
			KeyOpT.Text = "";
			PasOpT.Text = "";
		}
		else
		{
			((Form)this).Text = "Редагувати оператора";
			((Control)FioOpT).Enabled = true;
			((Control)InnOpT).Enabled = false;
			((Control)PasOpT).Enabled = true;
			((Control)KeyOpT).Enabled = false;
			KeyOpT.Text = tKeyOp;
			PasOpT.Text = "*********";
		}
	}
}
